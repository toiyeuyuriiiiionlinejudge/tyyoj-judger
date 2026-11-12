cmake_minimum_required(VERSION 3.20)
project(tyyoj_sandbox LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_compile_options(-Wall -Wextra -Wpedantic -O2)

add_executable(tyyoj_sandbox
    src/sandbox/main.cpp
)

target_include_directories(tyyoj_sandbox PRIVATE include)
