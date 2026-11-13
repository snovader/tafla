// Counts what each benchmark's call allocates: the allocations and
// reallocations, and the bytes that they ask for, over 100 calls after one,
// divided among them. A reallocation counts its new size, and freeing isn't
// counted.
//
// It replaces the global `operator new`, as C++ allows, which counts what the
// libraries allocate in C++, and defines `malloc()`, `calloc()` and
// `realloc()` over the system's, which counts what code compiled into this
// program allocates with them, as msgpack-cxx's header-only zone and sbuffer
// do. A library's own `malloc()` in another shared library isn't counted on
// macOS, whose shared libraries bind to the system's. Counting adds an
// increment to every allocation, so it runs apart from the timings.

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <new>
#include <string>

#include "suite.hpp"

#if defined(__APPLE__)
#include <malloc/malloc.h>
#endif

namespace {

struct Counts {
    std::uint64_t allocations = 0;
    std::uint64_t bytes = 0;
};

Counts counts;
bool counting = false;

void count(std::size_t size) {
    if (counting) {
        ++counts.allocations;
        counts.bytes += size;
    }
}

}  // namespace

// The system's allocator, which neither counts
#if defined(__APPLE__)
namespace {

void* system_malloc(std::size_t size) {
    return malloc_zone_malloc(malloc_default_zone(), size);
}

void* system_calloc(std::size_t count_of, std::size_t size) {
    return malloc_zone_calloc(malloc_default_zone(), count_of, size);
}

void* system_realloc(void* pointer, std::size_t size) {
    if (pointer == nullptr) {
        return system_malloc(size);
    }
    return malloc_zone_realloc(malloc_zone_from_ptr(pointer), pointer, size);
}

}  // namespace
#elif defined(__GLIBC__)
extern "C" {
void* __libc_malloc(std::size_t size);
void* __libc_calloc(std::size_t count_of, std::size_t size);
void* __libc_realloc(void* pointer, std::size_t size);
}

namespace {

void* system_malloc(std::size_t size) {
    return __libc_malloc(size);
}

void* system_calloc(std::size_t count_of, std::size_t size) {
    return __libc_calloc(count_of, size);
}

void* system_realloc(void* pointer, std::size_t size) {
    return __libc_realloc(pointer, size);
}

}  // namespace
#else
#error "Counting allocations needs macOS's or glibc's allocator"
#endif

extern "C" {

void* malloc(std::size_t size) {
    count(size);
    return system_malloc(size);
}

void* calloc(std::size_t count_of, std::size_t size) {
    count(count_of * size);
    return system_calloc(count_of, size);
}

void* realloc(void* pointer, std::size_t size) {
    count(size);
    return system_realloc(pointer, size);
}
}

void* operator new(std::size_t size) {
    count(size);
    if (void* pointer = system_malloc(size == 0 ? 1 : size)) {
        return pointer;
    }
    throw std::bad_alloc();
}

void* operator new[](std::size_t size) {
    return operator new(size);
}

void operator delete(void* pointer) noexcept {
    free(pointer);
}

void operator delete[](void* pointer) noexcept {
    free(pointer);
}

void operator delete(void* pointer, std::size_t) noexcept {
    free(pointer);
}

void operator delete[](void* pointer, std::size_t) noexcept {
    free(pointer);
}

int main() {
    using namespace tafla::benchmarks;

    const Inputs inputs;
    const auto all = benchmarks(inputs);
    constexpr std::size_t calls = 100;

    std::printf("Bytes allocated per call, and the allocations and reallocations that ask for them\n");
    for (const auto& benchmark : all) {
        std::printf("%s\n", benchmark.name.c_str());
        for (const auto& variant : benchmark.variants) {
            // One call first, so that what's allocated once, such as
            // protobuf's default instances, isn't counted
            variant.loop(1);

            counts = {};
            counting = true;
            variant.loop(calls);
            counting = false;

            const auto measured = counts;
            std::printf("  %-20s %12llu (%llu)\n", variant.name.c_str(),
                        static_cast<unsigned long long>(measured.bytes / calls),
                        static_cast<unsigned long long>(measured.allocations / calls));
        }
    }
    return 0;
}
