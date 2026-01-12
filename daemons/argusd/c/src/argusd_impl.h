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

#ifndef __ARGUSD_IMPL_H__
#define __ARGUSD_IMPL_H__

#include <condition_variable>
#include <map>
#include <memory>
#include <mutex>
#include <vector>

#include <grpc/grpc.h>
#include <grpc++/server_context.h>
#include <argus/v1/argus.grpc.pb.h>

namespace argusd {

/**
 * ArgusdImpl implements the v1 ArgusdService gRPC service interface.
 * It manages inotify watches for file integrity monitoring in containers.
 */
class ArgusdImpl final : public argus::v1::ArgusdService::Service {
public:
    explicit ArgusdImpl() = default;
    ~ArgusdImpl() override = default;

    // gRPC service methods (v1 API)
    grpc::Status CreateWatch(
        grpc::ServerContext *context,
        const argus::v1::CreateWatchRequest *request,
        argus::v1::CreateWatchResponse *response) override;

    grpc::Status DestroyWatch(
        grpc::ServerContext *context,
        const argus::v1::DestroyWatchRequest *request,
        google::protobuf::Empty *response) override;

    grpc::Status GetWatchState(
        grpc::ServerContext *context,
        const argus::v1::GetWatchStateRequest *request,
        grpc::ServerWriter<argus::v1::WatchState> *writer) override;

    grpc::Status StreamEvents(
        grpc::ServerContext *context,
        const argus::v1::StreamEventsRequest *request,
        grpc::ServerWriter<argus::v1::FileEvent> *writer) override;

    grpc::Status GetMetrics(
        grpc::ServerContext *context,
        const argus::v1::GetMetricsRequest *request,
        argus::v1::MetricsResponse *response) override;

private:
    // Internal watch state structure
    struct InternalWatchState {
        std::string watcher_name;
        std::string namespace_;
        std::string node_name;
        std::string pod_name;
        std::vector<int32_t> pids;
        int32_t watch_descriptors;
        std::chrono::system_clock::time_point created_at;
        bool paused = false;                              // Whether watcher is paused
        argus::v1::CreateWatchRequest stored_request;    // For auto-recreate on unpause
    };

    // Helper methods
    std::vector<int> getPidsFromRequest(std::shared_ptr<argus::v1::CreateWatchRequest> request) const;
    std::shared_ptr<InternalWatchState> findArgusdWatcherByPids(const std::string &nodeName, const std::vector<int> &pids) const;

    char **getPathArrayFromSubject(int pid, std::shared_ptr<argus::v1::WatchSubject> subject) const;
    char **getIgnoreArrayFromSubject(std::shared_ptr<argus::v1::WatchSubject> subject) const;
    std::string getTagListFromSubject(std::shared_ptr<argus::v1::WatchSubject> subject) const;
    uint32_t getEventMaskFromSubject(std::shared_ptr<argus::v1::WatchSubject> subject) const;
    uint32_t getFlagsFromSubject(std::shared_ptr<argus::v1::WatchSubject> subject) const;

    void createInotifyWatcher(
        const std::string &watcherName,
        const std::string &nodeName,
        const std::string &podName,
        std::shared_ptr<argus::v1::WatchSubject> subject,
        int pid, int sid, int subjectLen,
        const std::string &logFormat);

    void sendKillSignalToWatcher(std::shared_ptr<InternalWatchState> watcher) const;

    // State
    std::vector<std::shared_ptr<InternalWatchState>> watchers_;
    std::map<int, bool> doneMap_;
    std::condition_variable cv_;
    mutable std::mutex mux_;
};

} // namespace argusd

// C callback for logging inotify events
#ifdef __cplusplus
extern "C" {
#endif
void logArgusWatchEvent(struct arguswatch_event *awevent);
#ifdef __cplusplus
}
#endif

#endif // __ARGUSD_IMPL_H__
