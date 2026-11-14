// Counts what each builder row's call allocates: the allocations and
// reallocations, and the bytes they ask for, over 100 calls after one,
// divided among them, as `tafla_allocation` counts them. A reallocation
// counts its new size, and freeing isn't counted.
//
// It replaces the global `operator new`, which `tafla::TableBuilder`, `Bytes`
// and the prototype's segments allocate with, and defines `malloc()`,
// `calloc()` and `realloc()` over the system's, which the prototype's bytes
// grow with when they're reallocated, as `benchmarks/src/allocation.cpp`
// does.

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <new>

#include "builder_suite.hpp"

#if defined(__APPLE__)
#include <malloc/malloc.h>
#endif

namespace {

std::uint64_t allocations = 0;
std::uint64_t allocated = 0;
bool counting = false;

void count(std::size_t size) {
    if (counting) {
        ++allocations;
        allocated += size;
    }
}

}  // namespace

// The system's allocator, which doesn't count
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

void operator delete(void* pointer, std::size_t /*size*/) noexcept {
    free(pointer);
}

void operator delete[](void* pointer, std::size_t /*size*/) noexcept {
    free(pointer);
}

int main() {
    constexpr std::size_t calls = 100;
    const tafla::prototype::Inputs inputs;

    std::printf("Bytes allocated per call, and the allocations and reallocations that ask for them\n");
    for (const auto& benchmark : tafla::prototype::benchmarks(inputs)) {
        std::printf("%s\n", benchmark.name.c_str());
        for (const auto& variant : benchmark.variants) {
            variant.loop(1);

            allocations = 0;
            allocated = 0;
            counting = true;
            variant.loop(calls);
            counting = false;

            std::printf("  %-26s %12llu (%llu)\n", variant.name.c_str(),
                        static_cast<unsigned long long>(allocated / calls),
                        static_cast<unsigned long long>(allocations / calls));
        }
    }
    return 0;
}
