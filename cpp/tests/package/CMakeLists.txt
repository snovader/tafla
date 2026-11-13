# A project that uses the installed package, which CI builds to check that
# `find_package(tafla)` finds it after `cmake --install`

cmake_minimum_required(VERSION 3.25)

project(tafla_package_test LANGUAGES CXX)

find_package(tafla 0.0 REQUIRED)

add_executable(package_test main.cpp)
target_link_libraries(package_test PRIVATE tafla::tafla)
