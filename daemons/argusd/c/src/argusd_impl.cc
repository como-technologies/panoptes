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

#include <poll.h>
#include <sys/eventfd.h>
#include <sys/inotify.h>
#include <algorithm>
#include <chrono>
#include <condition_variable>
#include <functional>
#include <future>
#include <memory>
#include <mutex>
#include <regex>
#include <sstream>
#include <string>
#include <thread>

#include <fmt/format.h>
#include <glog/logging.h>
#include <grpc/grpc.h>
#include <grpc++/server_context.h>

#include "argusd_impl.h"

extern "C" {
#include <argusnotify.h>
#include <argusutil.h>
#include <container_runtime.h>
}

grpc::ServerWriter<argus::v1::FileEvent> *kMetricsWriter;

namespace argusd {

/**
 * CreateWatch is responsible for creating (or updating) an argus watcher. Find
 * list of PIDs from the request's container IDs list. With the list of PIDs,
 * create `inotify` watchers by spawning an argusnotify process that handles
 * the filesystem-level instructions.
 */
grpc::Status ArgusdImpl::CreateWatch(grpc::ServerContext *context [[maybe_unused]],
    const argus::v1::CreateWatchRequest *request,
    argus::v1::CreateWatchResponse *response) {

    auto pids = getPidsFromRequest(std::make_shared<argus::v1::CreateWatchRequest>(*request));
    if (pids.empty()) {
        return grpc::Status::CANCELLED;
    }

    // Find existing watcher by pid in case we need to update
    // `inotify_add_watcher` is designed to both add and modify depending on if
    // a fd exists already for this path.
    auto watcher = findArgusdWatcherByPids(request->node_name(), pids);

    // Check if paused - store config but skip creating watches
    if (request->paused()) {
        LOG(INFO) << "Watcher paused, storing config but skipping inotify setup ("
            << request->pod_name() << ":" << request->node_name() << ")";

        // Store or update the paused watcher state
        if (watcher == nullptr) {
            auto new_watcher = std::make_shared<InternalWatchState>();
            new_watcher->watcher_name = request->watcher_name();
            new_watcher->namespace_ = request->namespace_();
            new_watcher->node_name = request->node_name();
            new_watcher->pod_name = request->pod_name();
            new_watcher->pids.assign(pids.begin(), pids.end());
            new_watcher->watch_descriptors = 0;
            new_watcher->created_at = std::chrono::system_clock::now();
            new_watcher->paused = true;
            new_watcher->stored_request = *request;
            watchers_.push_back(new_watcher);
        } else {
            watcher->paused = true;
            watcher->stored_request = *request;
            // Stop existing watches if any
            sendKillSignalToWatcher(watcher);
        }

        response->set_node_name(request->node_name());
        response->set_pod_name(request->pod_name());
        response->set_watched_paths(0);
        response->set_paused(true);

        std::stringstream ss;
        ss << request->watcher_name() << "-" << request->pod_name();
        response->set_watch_id(ss.str());

        return grpc::Status::OK;
    }

    // Log appropriate message based on state
    if (watcher != nullptr && watcher->paused) {
        LOG(INFO) << "Watcher unpaused, recreating inotify watches ("
            << request->pod_name() << ":" << request->node_name() << ")";
        watcher->paused = false;
    } else if (watcher != nullptr) {
        LOG(INFO) << "Updating `inotify` watcher ("
            << request->pod_name() << ":" << request->node_name() << ")";
    } else {
        LOG(INFO) << "Starting `inotify` watcher ("
            << request->pod_name() << ":" << request->node_name() << ")";
    }

    if (watcher != nullptr && !watcher->paused) {
        // Stop existing watcher polling.
        sendKillSignalToWatcher(watcher);

        // Wait for all inotify threads to be finished and cleaned up.
        std::unique_lock<std::mutex> lock(mux_);
        cv_.wait_until(lock, std::chrono::system_clock::now() + std::chrono::seconds(2), [=] {
            for (const auto &it : doneMap_) {
                if (!it.second) {
                    return false;
                }
            }
            return true;
        });
    }

    response->set_node_name(request->node_name().c_str());
    response->set_pod_name(request->pod_name().c_str());

    int32_t watched_paths = 0;
    std::for_each(pids.cbegin(), pids.cend(), [&](const int pid) {
        int i = 0;
        // Reset done map flags.
        doneMap_[pid] = false;

        std::for_each(request->subjects().cbegin(), request->subjects().cend(), [&](const argus::v1::WatchSubject &subject) {
            createInotifyWatcher(request->watcher_name(), request->node_name(), request->pod_name(),
                std::make_shared<argus::v1::WatchSubject>(subject), pid, i, request->subjects_size(),
                request->log_format());
            ++i;
            watched_paths += subject.paths_size();
        });
    });

    response->set_watched_paths(watched_paths);
    response->set_paused(false);

    // Generate a watch ID
    std::stringstream ss;
    ss << request->watcher_name() << "-" << request->pod_name();
    response->set_watch_id(ss.str());

    if (watcher == nullptr) {
        // Store new watcher state
        auto new_watcher = std::make_shared<InternalWatchState>();
        new_watcher->watcher_name = request->watcher_name();
        new_watcher->namespace_ = request->namespace_();
        new_watcher->node_name = request->node_name();
        new_watcher->pod_name = request->pod_name();
        new_watcher->pids.assign(pids.begin(), pids.end());
        new_watcher->watch_descriptors = watched_paths;
        new_watcher->created_at = std::chrono::system_clock::now();
        new_watcher->paused = false;
        new_watcher->stored_request = *request;
        watchers_.push_back(new_watcher);
    } else {
        // Update existing watcher state
        watcher->watch_descriptors = watched_paths;
        watcher->stored_request = *request;
    }

    return grpc::Status::OK;
}

/**
 * DestroyWatch is responsible for deleting an argus watcher. Send kill signal
 * to the argusnotify poller to stop that child process.
 */
grpc::Status ArgusdImpl::DestroyWatch(grpc::ServerContext *context [[maybe_unused]],
    const argus::v1::DestroyWatchRequest *request,
    google::protobuf::Empty *response [[maybe_unused]]) {

    LOG(INFO) << "Stopping `inotify` watcher (" << request->pod_name() << ")";

    // Find watcher by name and pod
    auto it = std::find_if(watchers_.begin(), watchers_.end(),
        [request](const std::shared_ptr<InternalWatchState> &w) {
            return w->watcher_name == request->watcher_name() &&
                   w->pod_name == request->pod_name();
        });

    if (it != watchers_.end()) {
        sendKillSignalToWatcher(*it);
        watchers_.erase(it);
    }

    return grpc::Status::OK;
}

/**
 * GetWatchState periodically gets called by the Kubernetes controller and is
 * responsible for gathering the current watcher state to send back so the
 * controller can reconcile if any watchers need to be added or destroyed.
 */
grpc::Status ArgusdImpl::GetWatchState(grpc::ServerContext *context [[maybe_unused]],
    const argus::v1::GetWatchStateRequest *request,
    grpc::ServerWriter<argus::v1::WatchState> *writer) {

    for (const auto &watcher : watchers_) {
        // Filter by watcher_name and namespace if specified
        if (!request->watcher_name().empty() &&
            watcher->watcher_name != request->watcher_name()) {
            continue;
        }
        if (!request->namespace_().empty() &&
            watcher->namespace_ != request->namespace_()) {
            continue;
        }

        argus::v1::WatchState state;
        state.set_watcher_name(watcher->watcher_name);
        state.set_namespace_(watcher->namespace_);
        state.set_node_name(watcher->node_name);
        state.set_pod_name(watcher->pod_name);
        for (int32_t pid : watcher->pids) {
            state.add_pids(pid);
        }
        state.set_watch_descriptors(watcher->watch_descriptors);

        // Set created_at timestamp
        auto *ts = state.mutable_created_at();
        auto duration = watcher->created_at.time_since_epoch();
        auto seconds = std::chrono::duration_cast<std::chrono::seconds>(duration);
        auto nanos = std::chrono::duration_cast<std::chrono::nanoseconds>(duration - seconds);
        ts->set_seconds(seconds.count());
        ts->set_nanos(static_cast<int32_t>(nanos.count()));

        // Include config for operator comparison (query-first pattern)
        for (const auto &subject : watcher->stored_request.subjects()) {
            auto *s = state.add_subjects();
            for (const auto &path : subject.paths()) {
                s->add_paths(path);
            }
            for (int i = 0; i < subject.events_size(); i++) {
                s->add_events(subject.events(i));
            }
            for (const auto &ignore : subject.ignore()) {
                s->add_ignore(ignore);
            }
            s->set_recursive(subject.recursive());
            s->set_max_depth(subject.max_depth());
            s->set_only_dir(subject.only_dir());
            s->set_follow_move(subject.follow_move());
        }
        state.set_log_format(watcher->stored_request.log_format());
        state.set_paused(watcher->paused);

        if (!writer->Write(state)) {
            break;
        }
    }

    return grpc::Status::OK;
}

/**
 * StreamEvents is used to send the controller `inotify` events that occur on
 * this daemon by way of a gRPC stream.
 */
grpc::Status ArgusdImpl::StreamEvents(grpc::ServerContext *context [[maybe_unused]],
    const argus::v1::StreamEventsRequest *request [[maybe_unused]],
    grpc::ServerWriter<argus::v1::FileEvent> *writer) {

    kMetricsWriter = writer;

    std::condition_variable cv;
    std::mutex mux;
    std::unique_lock<std::mutex> lock(mux);
    // Keep alive so new events coming from argusnotify can be written to the
    // bidirectional gRPC stream.
    cv.wait(lock, [=] {
        return kMetricsWriter == nullptr;
    });

    return grpc::Status::OK;
}

/**
 * GetMetrics retrieves current metrics for monitoring purposes.
 */
grpc::Status ArgusdImpl::GetMetrics(grpc::ServerContext *context [[maybe_unused]],
    const argus::v1::GetMetricsRequest *request,
    argus::v1::MetricsResponse *response) {

    response->set_active_watches(static_cast<int32_t>(watchers_.size()));

    int32_t total_descriptors = 0;
    for (const auto &watcher : watchers_) {
        total_descriptors += watcher->watch_descriptors;

        // Filter by watcher_name if specified
        if (!request->watcher_name().empty() &&
            watcher->watcher_name != request->watcher_name()) {
            continue;
        }

        auto *metrics = response->add_watch_metrics();
        metrics->set_watcher_name(watcher->watcher_name);
        metrics->set_namespace_(watcher->namespace_);
    }
    response->set_total_watch_descriptors(total_descriptors);

    return grpc::Status::OK;
}

/**
 * Return list of PIDs looked up by container IDs from request.
 * Uses our container_runtime.c instead of libcontainer.
 */
std::vector<int> ArgusdImpl::getPidsFromRequest(std::shared_ptr<argus::v1::CreateWatchRequest> request) const {
    std::vector<int> pids;

    // First check if PIDs are directly provided
    if (request->pids_size() > 0) {
        for (int i = 0; i < request->pids_size(); ++i) {
            pids.push_back(request->pids(i));
        }
        return pids;
    }

    // Otherwise look up PIDs from container IDs
    for (const auto &cid : request->container_ids()) {
        pid_t pid = get_container_pid(cid.c_str());
        if (pid > 0) {
            pids.push_back(pid);
        }
    }
    return pids;
}

/**
 * Returns stored watcher that pertains to a list of PIDs on a specific node.
 */
std::shared_ptr<ArgusdImpl::InternalWatchState> ArgusdImpl::findArgusdWatcherByPids(
    const std::string &nodeName, const std::vector<int> &pids) const {

    auto it = find_if(watchers_.cbegin(), watchers_.cend(), [&](std::shared_ptr<InternalWatchState> watcher) {
        bool foundPid = false;
        for (const auto &pid : pids) {
            auto watcherPid = std::find_if(watcher->pids.cbegin(), watcher->pids.cend(),
                [&](int32_t p) { return p == pid; });
            foundPid = watcherPid != watcher->pids.cend();
        }
        return watcher->node_name == nodeName && foundPid;
    });
    if (it != watchers_.cend()) {
        return *it;
    }
    return nullptr;
}

/**
 * Returns array of char buffer paths to do the actual watch on given a
 * subject. These prepend /proc/{PID}/root on each path so we can monitor via
 * procfs directly to receive inode events.
 */
char **ArgusdImpl::getPathArrayFromSubject(const int pid, std::shared_ptr<argus::v1::WatchSubject> subject) const {
    std::vector<std::string> pathvec;
    std::for_each(subject->paths().cbegin(), subject->paths().cend(), [&](const std::string &path) {
        std::stringstream ss;
        ss << "/proc/" << pid << "/root" << path.c_str();
        pathvec.push_back(ss.str());
    });

    char **patharr = new char *[pathvec.size()];
    for (size_t i = 0; i < pathvec.size(); ++i) {
        patharr[i] = new char[pathvec[i].size() + 1];
        strcpy(patharr[i], pathvec[i].c_str());
    }
    return patharr;
}

/**
 * Returns array of char buffer paths to ignore given a subject.
 */
char **ArgusdImpl::getIgnoreArrayFromSubject(std::shared_ptr<argus::v1::WatchSubject> subject) const {
    char **patharr = new char *[subject->ignore_size()];
    size_t i = 0;
    std::for_each(subject->ignore().cbegin(), subject->ignore().cend(), [&](const std::string &path) {
        patharr[i] = new char[path.size() + 1];
        strcpy(patharr[i], path.c_str());
        ++i;
    });
    return patharr;
}

/**
 * Returns a comma-separated list of key=value pairs for a subject tag map.
 */
std::string ArgusdImpl::getTagListFromSubject(std::shared_ptr<argus::v1::WatchSubject> subject) const {
    std::string tags;
    for (const auto &tag : subject->tags()) {
        if (!tags.empty()) {
            tags += ",";
        }
        tags += tag.first + "=" + tag.second;
    }
    return tags;
}

/**
 * Returns a bitwise-OR combined event mask given a subject.
 */
uint32_t ArgusdImpl::getEventMaskFromSubject(std::shared_ptr<argus::v1::WatchSubject> subject) const {
    uint32_t mask = 0;
    std::for_each(subject->events().cbegin(), subject->events().cend(), [&](int event_int) {
        auto event = static_cast<argus::v1::InotifyEvent>(event_int);
        switch (event) {
            case argus::v1::INOTIFY_EVENT_ALL:          mask |= IN_ALL_EVENTS; break;
            case argus::v1::INOTIFY_EVENT_ACCESS:       mask |= IN_ACCESS; break;
            case argus::v1::INOTIFY_EVENT_ATTRIB:       mask |= IN_ATTRIB; break;
            case argus::v1::INOTIFY_EVENT_CLOSE_WRITE:  mask |= IN_CLOSE_WRITE; break;
            case argus::v1::INOTIFY_EVENT_CLOSE_NOWRITE: mask |= IN_CLOSE_NOWRITE; break;
            case argus::v1::INOTIFY_EVENT_CREATE:       mask |= IN_CREATE; break;
            case argus::v1::INOTIFY_EVENT_DELETE:       mask |= IN_DELETE; break;
            case argus::v1::INOTIFY_EVENT_DELETE_SELF:  mask |= IN_DELETE_SELF; break;
            case argus::v1::INOTIFY_EVENT_MODIFY:       mask |= IN_MODIFY; break;
            case argus::v1::INOTIFY_EVENT_MOVE_SELF:    mask |= IN_MOVE_SELF; break;
            case argus::v1::INOTIFY_EVENT_MOVED_FROM:   mask |= IN_MOVED_FROM; break;
            case argus::v1::INOTIFY_EVENT_MOVED_TO:     mask |= IN_MOVED_TO; break;
            case argus::v1::INOTIFY_EVENT_OPEN:         mask |= IN_OPEN; break;
            default: break;
        }
    });
    return mask;
}

/**
 * Returns a bitwise-OR combined flags given a subject.
 */
uint32_t ArgusdImpl::getFlagsFromSubject(std::shared_ptr<argus::v1::WatchSubject> subject) const {
    uint32_t flags = 0;
    if (subject->only_dir()) {
        flags |= AW_ONLYDIR;
    }
    if (subject->recursive()) {
        flags |= AW_RECURSIVE;
    }
    if (subject->follow_move()) {
        flags |= AW_FOLLOW;
    }
    return flags;
}

/**
 * Create child processes as background threads for spawning an argusnotify
 * watcher.
 */
void ArgusdImpl::createInotifyWatcher(const std::string &watcherName, const std::string &nodeName,
    const std::string &podName, std::shared_ptr<argus::v1::WatchSubject> subject, const int pid,
    const int sid, const int subjectLen, const std::string &logFormat) {

    std::packaged_task<int(const char *, const char *, const char *, int, int, unsigned int, const char **,
        unsigned int, const char **, uint32_t, uint32_t, int, const char *, const char *, arguswatch_logfn)>
        task(start_inotify_watcher);
    std::shared_future<int> result(task.get_future());
    std::thread taskThread(std::move(task),
        strdup(watcherName.c_str()),
        strdup(nodeName.c_str()),
        strdup(podName.c_str()),
        pid, sid,
        subject->paths_size(), const_cast<const char **>(getPathArrayFromSubject(pid, subject)),
        subject->ignore_size(), const_cast<const char **>(getIgnoreArrayFromSubject(subject)),
        getEventMaskFromSubject(subject),
        getFlagsFromSubject(subject),
        subject->max_depth(),
        strdup(getTagListFromSubject(subject).c_str()),
        strdup(logFormat.c_str()),
        logArgusWatchEvent);
    // Start as daemon process.
    taskThread.detach();

    // Once the argusnotify task begins we listen for a return status in a
    // separate, cleanup thread.
    int cnt = 0;
    std::thread cleanupThread([=](std::shared_future<int> res) mutable {
        res.wait();
        if (res.valid()) {
            if (++cnt == subjectLen) {
                doneMap_[pid] = true;
            }
            cv_.notify_one();
        }
    }, result);
    cleanupThread.detach();
}

/**
 * Sends a message over the anonymous pipe to stop the argusnotify poller.
 */
void ArgusdImpl::sendKillSignalToWatcher(std::shared_ptr<InternalWatchState> watcher) const {
    std::for_each(watcher->pids.cbegin(), watcher->pids.cend(), [&](const int32_t pid) {
        send_watcher_kill_signal(pid);
    });
}

} // namespace argusd

#ifdef __cplusplus
extern "C" {
#endif
void logArgusWatchEvent(struct arguswatch_event *awevent) {
    /**
     * Default logging format.
     *
     * @specifier pod      Name of the pod.
     * @specifier node     Name of the node.
     * @specifier event    `inotify` event that was observed.
     * @specifier path     Name of the directory path.
     * @specifier file     Name of the file.
     * @specifier ftype    Evaluates to "file" or "directory".
     * @specifier tags     List of custom tags in key=value comma-separated list.
     * @specifier sep      Placeholder for a "/" character (e.g. between path/file).
     */
    static const std::string kDefaultFormat = "{event} {ftype} '{path}{sep}{file}' ({pod}:{node}) {tags}";

    // Safely extract all string fields with null checks
    const char *watcherName = (awevent->watch && awevent->watch->name) ? awevent->watch->name : "unknown";
    const char *nodeName = (awevent->watch && awevent->watch->node_name) ? awevent->watch->node_name : "unknown";
    const char *podName = (awevent->watch && awevent->watch->pod_name) ? awevent->watch->pod_name : "unknown";
    const char *pathName = awevent->path_name ? awevent->path_name : "";
    const char *fileName = awevent->file_name ? awevent->file_name : "";
    const char *logFormat = (awevent->watch && awevent->watch->log_format && *awevent->watch->log_format)
        ? awevent->watch->log_format : nullptr;
    const char *tags = (awevent->watch && awevent->watch->tags && *awevent->watch->tags)
        ? awevent->watch->tags : nullptr;

    std::string maskStr;
    if (awevent->event_mask & IN_ACCESS)             maskStr = "ACCESS";
    else if (awevent->event_mask & IN_ATTRIB)        maskStr = "ATTRIB";
    else if (awevent->event_mask & IN_CLOSE_WRITE)   maskStr = "CLOSE_WRITE";
    else if (awevent->event_mask & IN_CLOSE_NOWRITE) maskStr = "CLOSE_NOWRITE";
    else if (awevent->event_mask & IN_CREATE)        maskStr = "CREATE";
    else if (awevent->event_mask & IN_DELETE)        maskStr = "DELETE";
    else if (awevent->event_mask & IN_DELETE_SELF)   maskStr = "DELETE_SELF";
    else if (awevent->event_mask & IN_MODIFY)        maskStr = "MODIFY";
    else if (awevent->event_mask & IN_MOVE_SELF)     maskStr = "MOVE_SELF";
    else if (awevent->event_mask & IN_MOVED_FROM)    maskStr = "MOVED_FROM";
    else if (awevent->event_mask & IN_MOVED_TO)      maskStr = "MOVED_TO";
    else if (awevent->event_mask & IN_OPEN)          maskStr = "OPEN";

    // Clean path by removing /proc/{pid}/root prefix
    std::string cleanPath = *pathName ? std::regex_replace(pathName, std::regex("/proc/[0-9]+/root"), "") : "";

    fmt::memory_buffer out;
    try {
        fmt::format_to(std::back_inserter(out),
            logFormat ? std::string(logFormat) : kDefaultFormat,
            fmt::arg("event", maskStr),
            fmt::arg("ftype", awevent->is_dir ? "directory" : "file"),
            fmt::arg("path", cleanPath),
            fmt::arg("file", fileName),
            fmt::arg("sep", *fileName ? "/" : ""),
            fmt::arg("pod", podName),
            fmt::arg("node", nodeName),
            fmt::arg("tags", tags ? tags : ""));
        LOG(INFO) << fmt::to_string(out);
    } catch (const std::exception &e) {
        LOG(WARNING) << "Malformed ArgusWatcher `.spec.logFormat`: \"" << e.what() << "\"";
        // Fallback to simple logging
        LOG(INFO) << maskStr << " " << (awevent->is_dir ? "directory" : "file")
                  << " '" << cleanPath << "' (" << podName << ":" << nodeName << ")";
    }

    if (kMetricsWriter != nullptr) {
        argus::v1::FileEvent event;

        // Set timestamp
        auto *ts = event.mutable_timestamp();
        auto now = std::chrono::system_clock::now();
        auto duration = now.time_since_epoch();
        auto seconds = std::chrono::duration_cast<std::chrono::seconds>(duration);
        auto nanos = std::chrono::duration_cast<std::chrono::nanoseconds>(duration - seconds);
        ts->set_seconds(seconds.count());
        ts->set_nanos(static_cast<int32_t>(nanos.count()));

        event.set_watcher_name(watcherName);
        event.set_node_name(nodeName);
        event.set_pod_name(podName);

        // Map mask to event type
        argus::v1::InotifyEvent event_type = argus::v1::INOTIFY_EVENT_UNSPECIFIED;
        if (awevent->event_mask & IN_ACCESS)             event_type = argus::v1::INOTIFY_EVENT_ACCESS;
        else if (awevent->event_mask & IN_ATTRIB)        event_type = argus::v1::INOTIFY_EVENT_ATTRIB;
        else if (awevent->event_mask & IN_CLOSE_WRITE)   event_type = argus::v1::INOTIFY_EVENT_CLOSE_WRITE;
        else if (awevent->event_mask & IN_CLOSE_NOWRITE) event_type = argus::v1::INOTIFY_EVENT_CLOSE_NOWRITE;
        else if (awevent->event_mask & IN_CREATE)        event_type = argus::v1::INOTIFY_EVENT_CREATE;
        else if (awevent->event_mask & IN_DELETE)        event_type = argus::v1::INOTIFY_EVENT_DELETE;
        else if (awevent->event_mask & IN_DELETE_SELF)   event_type = argus::v1::INOTIFY_EVENT_DELETE_SELF;
        else if (awevent->event_mask & IN_MODIFY)        event_type = argus::v1::INOTIFY_EVENT_MODIFY;
        else if (awevent->event_mask & IN_MOVE_SELF)     event_type = argus::v1::INOTIFY_EVENT_MOVE_SELF;
        else if (awevent->event_mask & IN_MOVED_FROM)    event_type = argus::v1::INOTIFY_EVENT_MOVED_FROM;
        else if (awevent->event_mask & IN_MOVED_TO)      event_type = argus::v1::INOTIFY_EVENT_MOVED_TO;
        else if (awevent->event_mask & IN_OPEN)          event_type = argus::v1::INOTIFY_EVENT_OPEN;
        event.set_event_type(event_type);

        event.set_path(cleanPath);
        if (*fileName) {
            event.set_filename(fileName);
        }
        event.set_is_directory(awevent->is_dir);

        if (!kMetricsWriter->Write(event)) {
            // Broken stream.
        }
    }
}
#ifdef __cplusplus
}; // extern "C"
#endif
