// The code generated from `schemas/shapes.tafla`'s fields and reads as the
// tests' descriptions of tables, which `TableBuilder` builds by hand and
// `ByteBuffer` reads, and which compare floats by their bits

#ifndef TAFLA_TEST_GENERATED_SHAPES_DESCRIBE_HPP
#define TAFLA_TEST_GENERATED_SHAPES_DESCRIBE_HPP

#include <string>
#include <vector>

#include "common/description.hpp"
#include "shapes/shapes.hpp"

namespace tafla::test::shapes_tables {

inline Layout headerless() {
    return Layout::headerless(OffsetType::uint32);
}

inline Field nested(std::vector<Field> fields) {
    return {TableField{{Source::builder, {headerless(), std::nullopt, std::move(fields)}}, std::nullopt}};
}

inline Table describe(const shapes::PointFields& point) {
    return {Layout{}, std::nullopt, {{point.x}, {point.y}, {point.id}, {std::string(point.name)}}};
}

inline Table describe(const shapes::DocumentFields& document) {
    const auto& columns = document.columns;
    return {
        Layout{true, true, OffsetType::uint32},
        shapes::Document::FORMAT_VERSION,
        {
            {document.x},
            {document.y},
            {std::string(document.name)},
            nested({
                {columns.base},
                {std::vector<std::uint32_t>(columns.keys.begin(), columns.keys.end())},
                {std::vector<float>(columns.a.begin(), columns.a.end())},
                {std::vector<float>(columns.b.begin(), columns.b.end())},
            }),
        },
    };
}

inline std::vector<Field> describe_item(const shapes::ItemFields& item) {
    return {{item.value}, nested({{item.pair.first}, {item.pair.second}})};
}

inline Table describe(const shapes::ItemListFields& list) {
    VectorField items;
    for (const auto& item : list.items) {
        items.items.push_back({Source::builder, {headerless(), std::nullopt, describe_item(item)}});
    }
    return {Layout{}, std::nullopt, {{items}}};
}

inline Table describe(const shapes::Sample& sample) {
    return {Layout{}, std::nullopt, {{sample.key}, {sample.a}, {sample.b}}};
}

inline Table read(const shapes::Point& point) {
    return {Layout{}, std::nullopt, {{point.x()}, {point.y()}, {point.id()}, {std::string(point.name())}}};
}

inline Table read(const shapes::Document& document) {
    const auto columns = document.columns();
    return {
        Layout{true, true, OffsetType::uint32},
        shapes::Document::FORMAT_VERSION,
        {
            {document.x()},
            {document.y()},
            {std::string(document.name())},
            nested(
                {{columns.base()}, {columns.keys().to_vector()}, {columns.a().to_vector()}, {columns.b().to_vector()}}),
        },
    };
}

inline std::vector<Field> read_item(const shapes::Item& item) {
    const auto pair = item.pair();
    return {{item.value()}, nested({{pair.first()}, {pair.second()}})};
}

/// Reads an item list's items through its getter, which reads every item at
/// once
inline Table read(const shapes::ItemList& list) {
    VectorField items;
    for (const auto& item : list.items()) {
        items.items.push_back({Source::builder, {headerless(), std::nullopt, read_item(item)}});
    }
    return {Layout{}, std::nullopt, {{items}}};
}

/// Reads an item list's items through its view, each by index or by
/// iterating
inline Table read_view(const shapes::ItemList& list, bool by_index) {
    const auto view = list.items_view();
    VectorField items;
    if (by_index) {
        for (std::size_t index = 0; index < view.size(); ++index) {
            items.items.push_back({Source::builder, {headerless(), std::nullopt, read_item(view.get(index))}});
        }
    } else {
        for (const auto item : view) {
            items.items.push_back({Source::builder, {headerless(), std::nullopt, read_item(item)}});
        }
    }
    return {Layout{}, std::nullopt, {{items}}};
}

}  // namespace tafla::test::shapes_tables

#endif
