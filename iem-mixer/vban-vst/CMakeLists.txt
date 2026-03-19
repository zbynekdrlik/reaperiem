cmake_minimum_required(VERSION 3.22)
project(VBANiem VERSION 1.0.0)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Add JUCE
add_subdirectory(JUCE)

# Define plugin
juce_add_plugin(VBANiem
    COMPANY_NAME "NewLevel"
    IS_SYNTH FALSE
    NEEDS_MIDI_INPUT FALSE
    NEEDS_MIDI_OUTPUT FALSE
    IS_MIDI_EFFECT FALSE
    EDITOR_WANTS_KEYBOARD_FOCUS FALSE
    COPY_PLUGIN_AFTER_BUILD FALSE
    PLUGIN_MANUFACTURER_CODE Riem
    PLUGIN_CODE Viem
    FORMATS VST3
    VST3_CATEGORIES "Fx" "Network"
    PRODUCT_NAME "VBAN IEM")

# Generate JUCE header
juce_generate_juce_header(VBANiem)

# Source files
target_sources(VBANiem PRIVATE
    src/PluginProcessor.cpp
    src/PluginEditor.cpp
    src/vban/VBANPacket.cpp
    src/vban/VBANSender.cpp
    src/audio/AudioRingBuffer.cpp
    src/ui/MeterComponent.cpp)

# Link JUCE modules
target_link_libraries(VBANiem
    PRIVATE
        juce::juce_audio_utils
        juce::juce_audio_processors
        juce::juce_gui_extra
        juce::juce_core
    PUBLIC
        juce::juce_recommended_config_flags
        juce::juce_recommended_warning_flags)

# Compile definitions
target_compile_definitions(VBANiem
    PUBLIC
        JUCE_WEB_BROWSER=0
        JUCE_USE_CURL=0
        JUCE_VST3_CAN_REPLACE_VST2=0
        JUCE_DISPLAY_SPLASH_SCREEN=0)

# Windows-specific settings
if(WIN32)
    target_link_libraries(VBANiem PRIVATE ws2_32)
endif()
