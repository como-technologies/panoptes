/**
 * MIT License
 *
 * Copyright (c) 2018 ClusterGarage
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 *
 * Updated for Panoptes v1 proto API by Como Technologies, LTD (2026).
 */

#include "janusd_impl.h"

#include <poll.h>
#include <sys/eventfd.h>
#include <sys/fanotify.h>
#include <algorithm>
#include <chrono>
#include <cstring>
#include <future>
#include <memory>
#include <regex>
#include <sstream>
#include <string>
#include <thread>

#include <fmt/format.h>
#include <glog/logging.h>
#include <grpc/grpc.h>
#include <grpc++/server_context.h>

extern "C" {
#include <janusnotify.h>
#include <janusutil.h>
#include <container_runtime.h>
}

namespace janusd {

// Global event writer for C callback
static grpc::ServerWriter<janus::v1::AccessEvent>* g_event_writer = nullptr;
static std::mutex g_event_writer_mux;

grpc::Status JanusdImpl::CreateGuard(
    grpc::ServerContext* context,
    const janus::v1::CreateGuardRequest* request,
    janus::v1::CreateGuardResponse* response) {

    // Get PIDs from container IDs or use provided PIDs
    std::vector<int> pids;
    if (request->pids_size() > 0) {
        pids.assign(request->pids().begin(), request->pids().end());
    } else {
        pids = getPidsFromContainerIds(request->container_ids());
    }

    if (pids.empty()) {
        return grpc::Status(grpc::StatusCode::INVALID_ARGUMENT,
            "No valid PIDs found from container IDs");
    }

    // Find existing guard by pid in case we need to update.
    // fanotify_mark is designed to both add and modify depending on if a fd
    // exists already for this path.
    auto guard = findGuardByPids(request->node_name(), pids);

    // Check if paused - store config but skip creating guards
    if (request->paused()) {
        LOG(INFO) << "Guard paused, storing config but skipping fanotify setup ("
            << request->pod_name() << ":" << request->node_name() << ")";

        // Store or update the paused guard state
        if (guard == nullptr) {
            auto new_guard = std::make_shared<InternalGuardState>();
            new_guard->guard_name = request->guard_name();
            new_guard->namespace_ = request->namespace_();
            new_guard->node_name = request->node_name();
            new_guard->pod_name = request->pod_name();
            new_guard->pids.assign(pids.begin(), pids.end());
            new_guard->created_at = std::chrono::system_clock::now();
            new_guard->paused = true;
            new_guard->enforcing = request->enforcing();
            new_guard->stored_request = *request;
            guards_.push_back(new_guard);
        } else {
            guard->paused = true;
            guard->enforcing = request->enforcing();
            guard->stored_request = *request;
            // Stop existing guards if any
            sendKillSignalToGuard(guard);
        }

        response->set_node_name(request->node_name());
        response->set_pod_name(request->pod_name());
        response->set_guarded_paths(0);
        response->set_paused(true);
        response->set_enforcing(request->enforcing());

        std::stringstream ss;
        ss << request->guard_name() << "-" << request->pod_name();
        response->set_guard_id(ss.str());

        return grpc::Status::OK;
    }

    // Log appropriate message based on state
    if (guard != nullptr && guard->paused) {
        LOG(INFO) << "Guard unpaused, recreating fanotify guards ("
            << request->pod_name() << ":" << request->node_name() << ")";
        guard->paused = false;
    } else if (guard != nullptr) {
        LOG(INFO) << "Updating `fanotify` guard ("
            << request->pod_name() << ":" << request->node_name() << ")";
    } else {
        LOG(INFO) << "Starting `fanotify` guard ("
            << request->pod_name() << ":" << request->node_name() << ")";
    }

    // Log enforcing mode
    bool enforcing = request->enforcing();
    LOG(INFO) << "Guard mode: " << (enforcing ? "enforcing" : "audit (dry-run)");

    if (guard != nullptr && !guard->paused) {
        // Stop existing guard polling
        sendKillSignalToGuard(guard);

        // Wait for fanotify threads to finish
        std::unique_lock<std::mutex> lock(mux_);
        cv_.wait_for(lock, std::chrono::seconds(2), [&guard] {
            return guard->process_eventfds.empty();
        });
    }

    response->set_node_name(request->node_name());
    response->set_pod_name(request->pod_name());

    std::vector<int32_t> process_eventfds;
    int guarded_paths = 0;

    for (int pid : pids) {
        int i = 0;
        for (const auto& subject : request->subjects()) {
            createFanotifyGuard(
                request->guard_name(),
                request->node_name(),
                request->pod_name(),
                subject,
                pid, i,
                process_eventfds,
                request->log_format(),
                enforcing);
            ++i;
            guarded_paths += subject.allow_size() + subject.deny_size();
        }
    }

    response->set_guarded_paths(guarded_paths);
    for (int32_t fd : process_eventfds) {
        response->add_process_eventfds(fd);
    }
    response->set_paused(false);
    response->set_enforcing(enforcing);

    // Generate a guard ID
    std::stringstream ss;
    ss << request->guard_name() << "-" << request->pod_name();
    response->set_guard_id(ss.str());

    if (guard == nullptr) {
        // Store new guard state
        auto new_guard = std::make_shared<InternalGuardState>();
        new_guard->guard_name = request->guard_name();
        new_guard->namespace_ = request->namespace_();
        new_guard->node_name = request->node_name();
        new_guard->pod_name = request->pod_name();
        new_guard->pids.assign(pids.begin(), pids.end());
        new_guard->process_eventfds = process_eventfds;
        new_guard->created_at = std::chrono::system_clock::now();
        new_guard->paused = false;
        new_guard->enforcing = enforcing;
        new_guard->guarded_paths = guarded_paths;
        new_guard->stored_request = *request;
        guards_.push_back(new_guard);
    } else {
        // Update existing guard
        guard->process_eventfds.insert(guard->process_eventfds.end(),
            process_eventfds.begin(), process_eventfds.end());
        guard->enforcing = enforcing;
        guard->guarded_paths = guarded_paths;
        guard->stored_request = *request;
    }

    return grpc::Status::OK;
}

grpc::Status JanusdImpl::DestroyGuard(
    grpc::ServerContext* context,
    const janus::v1::DestroyGuardRequest* request,
    google::protobuf::Empty* response) {

    LOG(INFO) << "Stopping `fanotify` guard (" << request->guard_name() << "/" << request->pod_name() << ")";

    // Find guard by name and pod
    auto it = std::find_if(guards_.begin(), guards_.end(),
        [request](const std::shared_ptr<InternalGuardState>& g) {
            return g->guard_name == request->guard_name() &&
                   g->pod_name == request->pod_name();
        });

    if (it != guards_.end()) {
        sendKillSignalToGuard(*it);
        guards_.erase(it);
    }

    return grpc::Status::OK;
}

grpc::Status JanusdImpl::GetGuardState(
    grpc::ServerContext* context,
    const janus::v1::GetGuardStateRequest* request,
    grpc::ServerWriter<janus::v1::GuardState>* writer) {

    for (const auto& guard : guards_) {
        // Filter by guard_name and namespace if specified
        if (!request->guard_name().empty() &&
            guard->guard_name != request->guard_name()) {
            continue;
        }
        if (!request->namespace_().empty() &&
            guard->namespace_ != request->namespace_()) {
            continue;
        }

        janus::v1::GuardState state;
        state.set_guard_name(guard->guard_name);
        state.set_namespace_(guard->namespace_);
        state.set_node_name(guard->node_name);
        state.set_pod_name(guard->pod_name);
        for (int32_t pid : guard->pids) {
            state.add_pids(pid);
        }
        for (int32_t fd : guard->process_eventfds) {
            state.add_process_eventfds(fd);
        }

        // Set created_at timestamp
        auto* ts = state.mutable_created_at();
        auto duration = guard->created_at.time_since_epoch();
        auto seconds = std::chrono::duration_cast<std::chrono::seconds>(duration);
        auto nanos = std::chrono::duration_cast<std::chrono::nanoseconds>(duration - seconds);
        ts->set_seconds(seconds.count());
        ts->set_nanos(static_cast<int32_t>(nanos.count()));

        // Include config for operator comparison (query-first pattern)
        for (const auto& subject : guard->stored_request.subjects()) {
            auto* s = state.add_subjects();
            for (const auto& path : subject.allow()) {
                s->add_allow(path);
            }
            for (const auto& path : subject.deny()) {
                s->add_deny(path);
            }
            for (int i = 0; i < subject.events_size(); i++) {
                s->add_events(subject.events(i));
            }
            s->set_only_dir(subject.only_dir());
            s->set_auto_allow_owner(subject.auto_allow_owner());
            s->set_audit(subject.audit());
            s->set_default_response(subject.default_response());
        }
        state.set_log_format(guard->stored_request.log_format());
        state.set_paused(guard->paused);
        state.set_enforcing(guard->enforcing);
        state.set_guarded_paths(guard->guarded_paths);

        if (!writer->Write(state)) {
            break;
        }
    }

    return grpc::Status::OK;
}

grpc::Status JanusdImpl::StreamAccessEvents(
    grpc::ServerContext* context,
    const janus::v1::StreamAccessEventsRequest* request,
    grpc::ServerWriter<janus::v1::AccessEvent>* writer) {

    {
        std::lock_guard<std::mutex> lock(g_event_writer_mux);
        g_event_writer = writer;
    }

    // Keep stream alive until client disconnects
    while (!context->IsCancelled()) {
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }

    {
        std::lock_guard<std::mutex> lock(g_event_writer_mux);
        g_event_writer = nullptr;
    }

    return grpc::Status::OK;
}

grpc::Status JanusdImpl::GetMetrics(
    grpc::ServerContext* context,
    const janus::v1::GetMetricsRequest* request,
    janus::v1::MetricsResponse* response) {

    response->set_active_guards(static_cast<int32_t>(guards_.size()));

    for (const auto& guard : guards_) {
        // Filter by guard_name if specified
        if (!request->guard_name().empty() &&
            guard->guard_name != request->guard_name()) {
            continue;
        }

        auto* metrics = response->add_guard_metrics();
        metrics->set_guard_name(guard->guard_name);
        metrics->set_namespace_(guard->namespace_);
    }

    return grpc::Status::OK;
}

std::vector<int> JanusdImpl::getPidsFromContainerIds(
    const google::protobuf::RepeatedPtrField<std::string>& container_ids) {

    std::vector<int> pids;
    for (const auto& cid : container_ids) {
        pid_t pid = get_container_pid(cid.c_str());
        if (pid > 0) {
            pids.push_back(pid);
        }
    }
    return pids;
}

std::shared_ptr<JanusdImpl::InternalGuardState> JanusdImpl::findGuardByPids(
    const std::string& node_name,
    const std::vector<int>& pids) {

    auto it = std::find_if(guards_.begin(), guards_.end(),
        [&](const std::shared_ptr<InternalGuardState>& guard) {
            if (guard->node_name != node_name) return false;
            for (int pid : pids) {
                auto found = std::find(guard->pids.begin(), guard->pids.end(), pid);
                if (found != guard->pids.end()) return true;
            }
            return false;
        });

    return it != guards_.end() ? *it : nullptr;
}

char** JanusdImpl::getPathArrayFromPaths(int pid,
    const google::protobuf::RepeatedPtrField<std::string>& paths) {

    std::vector<std::string> pathvec;
    for (const auto& path : paths) {
        std::stringstream ss;
        ss << "/proc/" << pid << "/root" << path;
        pathvec.push_back(ss.str());
    }

    char** patharr = new char*[pathvec.size()];
    for (size_t i = 0; i < pathvec.size(); ++i) {
        patharr[i] = new char[pathvec[i].size() + 1];
        strcpy(patharr[i], pathvec[i].c_str());
    }
    return patharr;
}

std::string JanusdImpl::getTagListFromSubject(const janus::v1::GuardSubject& subject) {
    std::string tags;
    for (const auto& tag : subject.tags()) {
        if (!tags.empty()) tags += ",";
        tags += tag.first + "=" + tag.second;
    }
    return tags;
}

uint32_t JanusdImpl::getEventMaskFromSubject(const janus::v1::GuardSubject& subject) {
    uint32_t mask = 0;
    for (const auto& event : subject.events()) {
        switch (event) {
            case janus::v1::FANOTIFY_EVENT_ALL:        mask |= FAN_ALL_PERM_EVENTS; break;
            case janus::v1::FANOTIFY_EVENT_ACCESS:     mask |= FAN_ACCESS_PERM; break;
            case janus::v1::FANOTIFY_EVENT_OPEN:       mask |= FAN_OPEN_PERM; break;
            case janus::v1::FANOTIFY_EVENT_OPEN_EXEC:  mask |= FAN_OPEN_EXEC_PERM; break;
            default: break;
        }
    }
    return mask;
}

void JanusdImpl::createFanotifyGuard(
    const std::string& guard_name,
    const std::string& node_name,
    const std::string& pod_name,
    const janus::v1::GuardSubject& subject,
    int pid, int sid,
    std::vector<int32_t>& process_eventfds,
    const std::string& log_format,
    bool enforcing) {

    // Create anonymous pipe to communicate with fanotify guard
    const int processfd = eventfd(0, EFD_CLOEXEC);
    if (processfd == EOF) {
        return;
    }
    process_eventfds.push_back(processfd);

    std::packaged_task<int(char*, int, int, char*, char*, unsigned int, char**,
        unsigned int, char**, uint32_t, bool, bool, bool, bool, int, char*, char*,
        void(struct janusguard_event*))> task(start_fanotify_guard);

    std::shared_future<int> result(task.get_future());

    std::thread taskThread(std::move(task),
        strdup(guard_name.c_str()),
        pid, sid,
        strdup(node_name.c_str()),
        strdup(pod_name.c_str()),
        subject.allow_size(),
        getPathArrayFromPaths(pid, subject.allow()),
        subject.deny_size(),
        getPathArrayFromPaths(pid, subject.deny()),
        getEventMaskFromSubject(subject),
        subject.only_dir(),
        subject.auto_allow_owner(),
        subject.audit(),
        enforcing,
        processfd,
        strdup(getTagListFromSubject(subject).c_str()),
        strdup(log_format.c_str()),
        logJanusGuardEvent);

    taskThread.detach();

    // Cleanup thread
    std::thread cleanupThread([this, result, node_name, pid, processfd]() mutable {
        result.wait();
        if (result.valid()) {
            auto guard = findGuardByPids(node_name, std::vector<int>{pid});
            if (guard != nullptr) {
                auto it = std::find(guard->process_eventfds.begin(),
                    guard->process_eventfds.end(), processfd);
                if (it != guard->process_eventfds.end()) {
                    guard->process_eventfds.erase(it);
                }
                cv_.notify_one();
            }
        }
    });
    cleanupThread.detach();
}

void JanusdImpl::sendKillSignalToGuard(std::shared_ptr<InternalGuardState> guard) {
    for (int32_t processfd : guard->process_eventfds) {
        send_guard_kill_signal(processfd);
    }
}

} // namespace janusd

#ifdef __cplusplus
extern "C" {
#endif

void logJanusGuardEvent(struct janusguard_event* jgevent) {
    /**
     * Default logging format.
     *
     * @specifier pod      Name of the pod.
     * @specifier node     Name of the node.
     * @specifier response Evaluates to "allow" or "deny".
     * @specifier event    `fanotify` event that was observed.
     * @specifier path     Name of the directory+file path.
     * @specifier ftype    Evaluates to "file" or "directory".
     * @specifier tags     List of custom tags in key=value comma-separated list.
     */
    const std::string kDefaultFormat = "<{response}> {event} {ftype} '{path}' ({pod}:{node}) {tags}";

    // Safely extract all string fields with null checks
    const char *guardName = (jgevent->guard && jgevent->guard->name) ? jgevent->guard->name : "unknown";
    const char *nodeName = (jgevent->guard && jgevent->guard->node_name) ? jgevent->guard->node_name : "unknown";
    const char *podName = (jgevent->guard && jgevent->guard->pod_name) ? jgevent->guard->pod_name : "unknown";
    const char *pathName = jgevent->path_name ? jgevent->path_name : "";
    const char *logFormat = (jgevent->guard && jgevent->guard->log_format && *jgevent->guard->log_format)
        ? jgevent->guard->log_format : nullptr;
    const char *tags = (jgevent->guard && jgevent->guard->tags && *jgevent->guard->tags)
        ? jgevent->guard->tags : nullptr;

    // Clean path by removing /proc/{pid}/root prefix
    std::string cleanPath = *pathName ? std::regex_replace(pathName, std::regex("/proc/[0-9]+/root"), "") : "";

    // Map fanotify mask to proto event type
    janus::v1::FanotifyEvent event_type = janus::v1::FANOTIFY_EVENT_UNSPECIFIED;
    std::string maskStr;

    if (jgevent->event_mask & FAN_ACCESS_PERM)        { event_type = janus::v1::FANOTIFY_EVENT_ACCESS; maskStr = "ACCESS_PERM"; }
    else if (jgevent->event_mask & FAN_OPEN_PERM)     { event_type = janus::v1::FANOTIFY_EVENT_OPEN; maskStr = "OPEN_PERM"; }
    else if (jgevent->event_mask & FAN_OPEN_EXEC_PERM){ event_type = janus::v1::FANOTIFY_EVENT_OPEN_EXEC; maskStr = "OPEN_EXEC_PERM"; }

    // Determine response type
    janus::v1::AccessResponse response_type = jgevent->allow ?
        janus::v1::ACCESS_RESPONSE_ALLOW : janus::v1::ACCESS_RESPONSE_DENY;

    // Log using fmt format
    fmt::memory_buffer out;
    try {
        fmt::format_to(std::back_inserter(out),
            logFormat ? std::string(logFormat) : kDefaultFormat,
            fmt::arg("response", jgevent->allow ? "ALLOW" : "DENY"),
            fmt::arg("event", maskStr),
            fmt::arg("ftype", jgevent->is_dir ? "directory" : "file"),
            fmt::arg("path", cleanPath),
            fmt::arg("pod", podName),
            fmt::arg("node", nodeName),
            fmt::arg("tags", tags ? tags : ""));
        LOG(INFO) << fmt::to_string(out);
    } catch(const std::exception& e) {
        LOG(WARNING) << "Malformed JanusGuard `.spec.logFormat`: \"" << e.what() << "\"";
        // Fallback to simple logging
        LOG(INFO) << (jgevent->allow ? "<ALLOW>" : "<DENY>") << " " << maskStr << " "
                  << (jgevent->is_dir ? "directory" : "file") << " '" << cleanPath
                  << "' (" << podName << ":" << nodeName << ")";
    }

    // Write to gRPC stream if available
    std::lock_guard<std::mutex> lock(janusd::g_event_writer_mux);
    if (janusd::g_event_writer != nullptr) {
        janus::v1::AccessEvent event;

        auto* ts = event.mutable_timestamp();
        auto now = std::chrono::system_clock::now();
        auto duration = now.time_since_epoch();
        auto seconds = std::chrono::duration_cast<std::chrono::seconds>(duration);
        auto nanos = std::chrono::duration_cast<std::chrono::nanoseconds>(duration - seconds);
        ts->set_seconds(seconds.count());
        ts->set_nanos(static_cast<int32_t>(nanos.count()));

        event.set_guard_name(guardName);
        event.set_node_name(nodeName);
        event.set_pod_name(podName);
        event.set_event_type(event_type);
        event.set_path(cleanPath);
        event.set_response(response_type);
        event.set_is_directory(jgevent->is_dir);

        janusd::g_event_writer->Write(event);
    }
}

#ifdef __cplusplus
}
#endif
