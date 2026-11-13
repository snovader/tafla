// Ways to read the items of tafla's vector of nested tables and of its array
// of structs, which tafla and tafla-gen write alike.
//
// Each reads the last item alone, which a reader can find without reading the
// others, and every item. It sums the three float32s of the items it reads as
// doubles, as the TypeScript comparison adds them, so that it returns a number
// rather than collecting items of its own.

#ifndef TAFLA_BENCHMARKS_ITEM_ACCESS_HPP
#define TAFLA_BENCHMARKS_ITEM_ACCESS_HPP

#include <cstdint>
#include <span>
#include <stdexcept>
#include <string>

#include <tafla/tafla.hpp>

#include "codecs/tafla.hpp"
#include "tafla/workloads.hpp"
#include "workloads.hpp"

namespace tafla::benchmarks {

/// `ByteBuffer`'s getters by hand: each item in place, from
/// `Vector::get_in_place()` of the last item or `items_in_place()`, and
/// `get_struct_array()`'s bytes
struct TaflaItems {
    static constexpr std::string_view name = "tafla";

    [[nodiscard]] double last_table(std::span<const std::uint8_t> bytes) const {
        const ByteBuffer buffer(bytes, TableLayout());
        const auto vector = buffer.get_vector(buffer.root_offset(0));
        return sum_table(vector.get_in_place(vector.size() - 1));
    }

    [[nodiscard]] double every_table(std::span<const std::uint8_t> bytes) const {
        const ByteBuffer buffer(bytes, TableLayout());
        const auto vector = buffer.get_vector(buffer.root_offset(0));

        double sum = 0;
        for (const auto item : vector.items_in_place()) {
            sum += sum_table(item);
        }
        return sum;
    }

    [[nodiscard]] double last_struct(std::span<const std::uint8_t> bytes) const {
        const ByteBuffer buffer(bytes, TableLayout());
        const auto structs = buffer.get_struct_array(buffer.root_offset(0), Tafla::kItemSize);
        return sum_struct(structs.data() + structs.size() - Tafla::kItemSize);
    }

    [[nodiscard]] double every_struct(std::span<const std::uint8_t> bytes) const {
        const ByteBuffer buffer(bytes, TableLayout());
        const auto structs = buffer.get_struct_array(buffer.root_offset(0), Tafla::kItemSize);

        double sum = 0;
        for (std::size_t start = 0; start < structs.size(); start += Tafla::kItemSize) {
            sum += sum_struct(structs.data() + start);
        }
        return sum;
    }

    static double sum_table(std::span<const std::uint8_t> bytes) {
        const ByteBuffer item(bytes, TableLayout());
        const auto pair = item.root_offset(1);
        return sum_item(item.get_float32(item.root_offset(0)), item.get_float32(item.field_offset(pair, 0)),
                        item.get_float32(item.field_offset(pair, 1)));
    }

    static double sum_struct(const std::uint8_t* bytes) {
        return sum_item(load_le<float>(bytes), load_le<float>(bytes + 4), load_le<float>(bytes + 8));
    }
};

inline double sum_of(const ::workloads::Item& item) {
    const auto pair = item.pair();
    return sum_item(item.value(), pair.first(), pair.second());
}

inline double sum_of(const ::workloads::Triple& triple) {
    return sum_item(triple.value, triple.first, triple.second);
}

/// The generated getters, `items()`, which read every item into a vector
/// first
struct TaflaGenItems {
    static constexpr std::string_view name = "tafla-gen";

    [[nodiscard]] double last_table(std::span<const std::uint8_t> bytes) const {
        return sum_of(::workloads::ItemList(bytes).items().back());
    }

    [[nodiscard]] double every_table(std::span<const std::uint8_t> bytes) const {
        double sum = 0;
        for (const auto& item : ::workloads::ItemList(bytes).items()) {
            sum += sum_of(item);
        }
        return sum;
    }

    [[nodiscard]] double last_struct(std::span<const std::uint8_t> bytes) const {
        return sum_of(::workloads::Triples(bytes).items().back());
    }

    [[nodiscard]] double every_struct(std::span<const std::uint8_t> bytes) const {
        double sum = 0;
        for (const auto& triple : ::workloads::Triples(bytes).items()) {
            sum += sum_of(triple);
        }
        return sum;
    }
};

/// The generated views, `items_view()`, whose `get(i)` reads one item and whose
/// iterators read every item
struct TaflaGenViewItems {
    static constexpr std::string_view name = "tafla-gen view";

    [[nodiscard]] double last_table(std::span<const std::uint8_t> bytes) const {
        const auto items = ::workloads::ItemList(bytes).items_view();
        return sum_of(items.get(items.size() - 1));
    }

    [[nodiscard]] double every_table(std::span<const std::uint8_t> bytes) const {
        double sum = 0;
        for (const auto item : ::workloads::ItemList(bytes).items_view()) {
            sum += sum_of(item);
        }
        return sum;
    }

    [[nodiscard]] double last_struct(std::span<const std::uint8_t> bytes) const {
        const auto items = ::workloads::Triples(bytes).items_view();
        return sum_of(items.at(items.size() - 1));
    }

    [[nodiscard]] double every_struct(std::span<const std::uint8_t> bytes) const {
        double sum = 0;
        for (const auto triple : ::workloads::Triples(bytes).items_view()) {
            sum += sum_of(triple);
        }
        return sum;
    }
};

/// Checks that `access` sums the last item and every item of the list's
/// bytes to the sums of `list`'s items, with the same bits
template <class Access>
void check_items(const Access& access, const List& list, std::span<const std::uint8_t> tables,
                 std::span<const std::uint8_t> structs) {
    const auto& last = list.items.back();
    const auto last_sum = sum_item(last.value, last.first, last.second);
    double every_sum = 0;
    for (const auto& item : list.items) {
        every_sum += sum_item(item.value, item.first, item.second);
    }

    const std::pair<double, double> sums[] = {
        {access.last_table(tables), last_sum},
        {access.every_table(tables), every_sum},
        {access.last_struct(structs), last_sum},
        {access.every_struct(structs), every_sum},
    };
    for (const auto& [sum, expected] : sums) {
        if (!same_bits(sum, expected)) {
            throw std::logic_error(std::string(Access::name) + " summed the items as " + std::to_string(sum) +
                                   " rather than " + std::to_string(expected));
        }
    }
}

}  // namespace tafla::benchmarks

#endif
