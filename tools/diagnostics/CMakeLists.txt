cmake_minimum_required(VERSION 3.20)
project(media_diagnostics LANGUAGES CXX)
add_executable(audio-meter ../audio-meter.cpp)
target_link_libraries(audio-meter PRIVATE ole32 uuid)
set_property(TARGET audio-meter PROPERTY MSVC_RUNTIME_LIBRARY "MultiThreaded")

add_executable(opengl-probe ../opengl-probe.cpp)
target_link_libraries(opengl-probe PRIVATE opengl32 gdi32 user32)
set_property(TARGET opengl-probe PROPERTY MSVC_RUNTIME_LIBRARY "MultiThreaded")
