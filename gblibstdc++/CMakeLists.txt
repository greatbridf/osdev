cmake_minimum_required(VERSION 3.15)
project(gblibstdc++)

set(CMAKE_CXX_STANDARD 20)

add_library(gblibstdc++ STATIC src/stdc++.cpp)

file(GLOB_RECURSE GBSTDLIBCPP_PUBLIC_HEADERS ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_include_directories(gblibstdc++ PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
set_target_properties(gblibstdc++ PROPERTIES PUBLIC_HEADER "${GBSTDLIBCPP_PUBLIC_HEADERS}")
