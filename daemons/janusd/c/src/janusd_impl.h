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

#ifndef __JANUSD_IMPL_H__
#define __JANUSD_IMPL_H__

#include <condition_variable>
#include <map>
#include <memory>
#include <mutex>
#include <vector>

#include <grpc/grpc.h>
#include <grpc++/server_context.h>
#include <janus/v1/janus.grpc.pb.h>

namespace janusd {

/**
 * JanusdImpl implements the JanusdService gRPC service interface.
 * It manages fanotify guards for file access control in containers.
 */
class JanusdImpl final : public janus::v1::JanusdService::Service {
public:
    explicit JanusdImpl() = default;
    ~JanusdImpl() override = default;

    // gRPC service methods
    grpc::Status CreateGuard(
        grpc::ServerContext* context,
        const janus::v1::CreateGuardRequest* request,
        janus::v1::CreateGuardResponse* response) override;

    grpc::Status DestroyGuard(
        grpc::ServerContext* context,
        const janus::v1::DestroyGuardRequest* request,
        google::protobuf::Empty* response) override;

    grpc::Status GetGuardState(
        grpc::ServerContext* context,
        const janus::v1::GetGuardStateRequest* request,
        grpc::ServerWriter<janus::v1::GuardState>* writer) override;

    grpc::Status StreamAccessEvents(
        grpc::ServerContext* context,
        const janus::v1::StreamAccessEventsRequest* request,
        grpc::ServerWriter<janus::v1::AccessEvent>* writer) override;

    grpc::Status GetMetrics(
        grpc::ServerContext* context,
        const janus::v1::GetMetricsRequest* request,
        janus::v1::MetricsResponse* response) override;

private:
    // Internal guard state structure
    struct InternalGuardState {
        std::string guard_name;
        std::string namespace_;
        std::string node_name;
        std::string pod_name;
        std::vector<int32_t> pids;
        std::vector<int32_t> process_eventfds;
        std::chrono::system_clock::time_point created_at;
        bool paused = false;                              // Whether guard is paused
        bool enforcing = false;                           // Whether guard is enforcing (vs audit mode)
        int32_t guarded_paths = 0;                        // Number of guarded paths
        janus::v1::CreateGuardRequest stored_request;    // For auto-recreate on unpause
    };

    // Helper methods
    std::vector<int> getPidsFromContainerIds(const google::protobuf::RepeatedPtrField<std::string>& container_ids);
    std::shared_ptr<InternalGuardState> findGuardByPids(const std::string& node_name, const std::vector<int>& pids);

    char** getPathArrayFromPaths(int pid, const google::protobuf::RepeatedPtrField<std::string>& paths);
    std::string getTagListFromSubject(const janus::v1::GuardSubject& subject);
    uint32_t getEventMaskFromSubject(const janus::v1::GuardSubject& subject);

    void createFanotifyGuard(
        const std::string& guard_name,
        const std::string& node_name,
        const std::string& pod_name,
        const janus::v1::GuardSubject& subject,
        int pid, int sid,
        std::vector<int32_t>& process_eventfds,
        const std::string& log_format,
        bool enforcing);

    void sendKillSignalToGuard(std::shared_ptr<InternalGuardState> guard);

    // State
    std::vector<std::shared_ptr<InternalGuardState>> guards_;
    std::condition_variable cv_;
    std::mutex mux_;

    // Event streaming
    grpc::ServerWriter<janus::v1::AccessEvent>* event_writer_ = nullptr;
    std::mutex event_writer_mux_;
};

} // namespace janusd

// C callback for logging fanotify events
#ifdef __cplusplus
extern "C" {
#endif
void logJanusGuardEvent(struct janusguard_event* jgevent);
#ifdef __cplusplus
}
#endif

#endif // __JANUSD_IMPL_H__
