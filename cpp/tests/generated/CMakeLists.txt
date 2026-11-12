# The tests of the code that tafla-gen generates from the example schemas in
# schemas/, whose golden files lie in a directory of each schema's name here

foreach(test generated_all_types_test generated_fixed_fields_test generated_layouts_test generated_shapes_test shapes_cases_test)
    add_executable(${test} ${test}.cpp)
    target_include_directories(${test} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
    target_link_libraries(${test} PRIVATE tafla_test_common GTest::gtest_main)
    gtest_discover_tests(${test} DISCOVERY_TIMEOUT 60 PROPERTIES ENVIRONMENT "RC_PARAMS=max_success=256")
endforeach()
