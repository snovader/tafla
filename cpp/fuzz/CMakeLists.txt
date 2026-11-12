# libFuzzer targets, which mirror rust/fuzz and need a Clang with libFuzzer,
# such as Homebrew's or a Linux distribution's: Apple's Clang has none. The
# conformance target reads with the tests' descriptions, so the tests are
# built too.

foreach(target byte_buffer conformance)
    add_executable(fuzz_${target} ${target}.cpp)
    target_link_libraries(fuzz_${target} PRIVATE tafla::tafla tafla_warnings)
    target_link_options(fuzz_${target} PRIVATE -fsanitize=fuzzer)
endforeach()

target_link_libraries(fuzz_conformance PRIVATE tafla_test_common)
