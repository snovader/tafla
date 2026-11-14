# The prototype of a table builder that writes each field when it's set,
# which README.md describes: its tests, which check it against
# `tafla::TableBuilder`, and, with the benchmarks, its timings beside it.

add_library(tafla_prototype INTERFACE)
target_include_directories(tafla_prototype INTERFACE ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(tafla_prototype INTERFACE tafla::tafla tafla_warnings)

if(TAFLA_BUILD_TESTS)
    add_executable(direct_table_builder_test direct_table_builder_test.cpp)
    target_link_libraries(direct_table_builder_test PRIVATE tafla_prototype tafla_test_common GTest::gtest_main)
    # The comparison's workloads and its tafla codec, which need no other library
    target_include_directories(direct_table_builder_test PRIVATE ${PROJECT_SOURCE_DIR}/benchmarks/src)
    gtest_discover_tests(direct_table_builder_test DISCOVERY_TIMEOUT 60 PROPERTIES ENVIRONMENT
                         "RC_PARAMS=max_success=256")

    add_executable(direct_table_builder_exception_safety_test direct_table_builder_exception_safety_test.cpp)
    target_link_libraries(direct_table_builder_exception_safety_test PRIVATE tafla_prototype GTest::gtest_main)
    gtest_discover_tests(direct_table_builder_exception_safety_test DISCOVERY_TIMEOUT 60)
endif()

if(TAFLA_BUILD_BENCHMARKS)
    foreach(program builder_comparison builder_allocation)
        add_executable(tafla_${program} ${program}.cpp)
        target_include_directories(tafla_${program} PRIVATE ${PROJECT_SOURCE_DIR}/benchmarks/src)
        target_link_libraries(tafla_${program} PRIVATE tafla_prototype)
    endforeach()
endif()
