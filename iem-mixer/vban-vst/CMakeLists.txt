cmake_minimum_required(VERSION 3.22)
project(VBANiem VERSION 1.0.0)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Add JUCE
add_subdirectory(JUCE)

# Fetch libopus for Opus encoding
include(FetchContent)
FetchContent_Declare(opus
    GIT_REPOSITORY https://github.com/xiph/opus.git
    GIT_TAG v1.5.2)
set(OPUS_INSTALL OFF CACHE BOOL "" FORCE)
set(OPUS_BUILD_PROGRAMS OFF CACHE BOOL "" FORCE)
set(OPUS_BUILD_TESTING OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(opus)

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
    src/opus/OpusEncoderWrapper.cpp
    src/opus/OpusSender.cpp
    src/ui/MeterComponent.cpp)

# Include opus headers
target_include_directories(VBANiem PRIVATE ${opus_SOURCE_DIR}/include)

# Link JUCE modules + libopus
target_link_libraries(VBANiem
    PRIVATE
        juce::juce_audio_utils
        juce::juce_audio_processors
        juce::juce_gui_extra
        juce::juce_core
        opus
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

# OIEM Receive plugin — receives talkback audio from server (#123)
juce_add_plugin(OIEMReceive
    COMPANY_NAME "NewLevel"
    IS_SYNTH FALSE
    NEEDS_MIDI_INPUT FALSE
    NEEDS_MIDI_OUTPUT FALSE
    IS_MIDI_EFFECT FALSE
    EDITOR_WANTS_KEYBOARD_FOCUS FALSE
    COPY_PLUGIN_AFTER_BUILD FALSE
    PLUGIN_MANUFACTURER_CODE Riem
    PLUGIN_CODE Oirv
    FORMATS VST3
    VST3_CATEGORIES "Fx" "Network"
    PRODUCT_NAME "OIEM Receive")

juce_generate_juce_header(OIEMReceive)

target_sources(OIEMReceive PRIVATE
    src/OIEMReceiveProcessor.cpp
    src/OIEMReceiveEditor.cpp
    src/opus/OpusDecoderWrapper.cpp
    src/opus/OpusReceiver.cpp
    src/ui/MeterComponent.cpp)

target_include_directories(OIEMReceive PRIVATE ${opus_SOURCE_DIR}/include)

target_link_libraries(OIEMReceive
    PRIVATE
        juce::juce_audio_utils
        juce::juce_audio_processors
        juce::juce_gui_extra
        juce::juce_core
        opus
    PUBLIC
        juce::juce_recommended_config_flags
        juce::juce_recommended_warning_flags)

target_compile_definitions(OIEMReceive
    PUBLIC
        JUCE_WEB_BROWSER=0
        JUCE_USE_CURL=0
        JUCE_VST3_CAN_REPLACE_VST2=0
        JUCE_DISPLAY_SPLASH_SCREEN=0)

if(WIN32)
    target_link_libraries(OIEMReceive PRIVATE ws2_32)
endif()
