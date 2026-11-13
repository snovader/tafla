// The exceptions that reads and writes throw.

#ifndef TAFLA_ERRORS_HPP
#define TAFLA_ERRORS_HPP

#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>

#include "element.hpp"
#include "layout.hpp"

namespace tafla {

/// The base of tafla's exceptions, `DecodeError` and `BuildError`
class Error : public std::runtime_error {
public:
    using std::runtime_error::runtime_error;
};

/// Thrown when bytes can't be read as the table, array, string, struct,
/// vector or value that a read expects.
///
/// Every read checks its range against the supplied bytes before it returns,
/// so a truncated or corrupted buffer throws this rather than returning
/// values from outside them. Strings must also be valid UTF-8.
///
/// Checks can't tell a value that was corrupted within range from a real one,
/// such as an offset that points at another field. Formats that need that
/// guarantee can add a checksum.
class DecodeError : public Error {
public:
    enum class Kind : std::uint8_t {
        /// The read of `byte_length()` bytes at `position()` would leave the
        /// `available()` bytes
        out_of_range,
        /// The string at `position()` isn't valid UTF-8
        invalid_utf8,
        /// The vector item at `position()` ends before it starts, at `end()`.
        /// An item starts where the item before it ends, and the first after
        /// the ends.
        item_ends_before_start,
        /// The header carries format `version()`, which the reader can't
        /// read, since it reads version `supported()`. `ByteBuffer` doesn't
        /// compare versions: a reader throws this when
        /// `ByteBuffer::version()` isn't its own.
        unsupported_version,
    };

    static DecodeError out_of_range(std::size_t position, std::uint64_t byte_length, std::size_t available) {
        auto message = "Read exceeds the supplied buffer: " + std::to_string(byte_length) + " bytes at " +
                       std::to_string(position) + " of " + std::to_string(available);
        return {Kind::out_of_range, message, position, byte_length, available, 0, 0};
    }

    static DecodeError invalid_utf8(std::size_t position) {
        auto message = "The string at " + std::to_string(position) + " isn't valid UTF-8";
        return {Kind::invalid_utf8, message, position, 0, 0, 0, 0};
    }

    static DecodeError item_ends_before_start(std::size_t start, std::size_t end) {
        auto message =
            "The vector item at " + std::to_string(start) + " ends before it starts, at " + std::to_string(end);
        return {Kind::item_ends_before_start, message, start, 0, 0, end, 0};
    }

    static DecodeError unsupported_version(std::uint32_t version, std::uint32_t supported) {
        auto message = "Unsupported format version " + std::to_string(version) + "; the reader reads version " +
                       std::to_string(supported);
        return {Kind::unsupported_version, message, 0, 0, 0, version, supported};
    }

    [[nodiscard]] Kind kind() const noexcept {
        return kind_;
    }

    /// Where the read, string or vector item starts
    [[nodiscard]] std::size_t position() const noexcept {
        return position_;
    }

    /// The bytes that an out-of-range read would read
    [[nodiscard]] std::uint64_t byte_length() const noexcept {
        return byte_length_;
    }

    /// The length of the bytes that an out-of-range read was given
    [[nodiscard]] std::size_t available() const noexcept {
        return available_;
    }

    /// Where a vector item that ends before it starts ends
    [[nodiscard]] std::size_t end() const noexcept {
        return static_cast<std::size_t>(second_);
    }

    /// The version that an unsupported version's header carries
    [[nodiscard]] std::uint32_t version() const noexcept {
        return static_cast<std::uint32_t>(second_);
    }

    /// The version that the reader of an unsupported version reads
    [[nodiscard]] std::uint32_t supported() const noexcept {
        return supported_;
    }

private:
    DecodeError(Kind kind, const std::string& message, std::size_t position, std::uint64_t byte_length,
                std::size_t available, std::uint64_t second, std::uint32_t supported)
        : Error(message),
          kind_(kind),
          position_(position),
          byte_length_(byte_length),
          available_(available),
          second_(second),
          supported_(supported) {}

    Kind kind_;
    std::size_t position_;
    std::uint64_t byte_length_;
    std::size_t available_;
    /// A vector item's end, or a header's version
    std::uint64_t second_;
    std::uint32_t supported_;
};

/// Thrown when a `TableBuilder`, or a `TableSizer` and its `TableWriter`,
/// can't write a table, whatever the values' bytes. A setter of
/// `TableBuilder` that throws leaves the builder as it was.
class BuildError : public Error {
public:
    enum class Kind : std::uint8_t {
        /// The version doesn't fit in the layout's header slots
        version_too_large,
        /// The header of `fields()` offsets alone is longer than the layout's
        /// offsets address
        header_too_long,
        /// A field was set on a table that already holds its `fields()`
        /// fields, or written to a `TableWriter` at an index past them
        too_many_fields,
        /// The field at `index()` would make the table `length()` bytes,
        /// longer than the layout's offsets address
        too_long,
        /// The table was written, or nested, with `set()` of its `fields()`
        /// fields set
        incomplete,
        /// The `length()` bytes of an array of structs aren't a whole number
        /// of `size()`-byte structs, or `size()` is 0
        not_whole_structs,
        /// The fields that a `TableWriter` wrote differ from those that its
        /// `TableSizer` added: they wrote `written()` of the table's
        /// `length()` bytes, in all or before the first write that the table
        /// doesn't hold
        not_as_sized,
    };

    /// What a `BuildError` holds, which sizers and writers keep until they
    /// throw it
    struct Details {
        Kind kind;
        OffsetType offset_type = OffsetType::uint32;
        std::uint64_t first = 0;
        std::uint64_t second = 0;
    };

    explicit BuildError(const Details& details) : Error(message_of(details)), details_(details) {}

    static BuildError version_too_large(std::uint32_t version, OffsetType offset_type) {
        return BuildError({Kind::version_too_large, offset_type, version, 0});
    }

    static BuildError header_too_long(std::size_t fields, OffsetType offset_type) {
        return BuildError({Kind::header_too_long, offset_type, fields, 0});
    }

    static BuildError too_many_fields(std::size_t fields) {
        return BuildError({Kind::too_many_fields, OffsetType::uint32, fields, 0});
    }

    static BuildError too_long(std::size_t index, std::uint64_t length, OffsetType offset_type) {
        return BuildError({Kind::too_long, offset_type, index, length});
    }

    static BuildError incomplete(std::size_t set, std::size_t fields) {
        return BuildError({Kind::incomplete, OffsetType::uint32, set, fields});
    }

    static BuildError not_whole_structs(std::size_t length, std::size_t size) {
        return BuildError({Kind::not_whole_structs, OffsetType::uint32, length, size});
    }

    static BuildError not_as_sized(std::size_t written, std::size_t length) {
        return BuildError({Kind::not_as_sized, OffsetType::uint32, written, length});
    }

    [[nodiscard]] Kind kind() const noexcept {
        return details_.kind;
    }

    /// The offset type that a version, header or table didn't fit in
    [[nodiscard]] OffsetType offset_type() const noexcept {
        return details_.offset_type;
    }

    /// The version too large for its slot
    [[nodiscard]] std::uint32_t version() const noexcept {
        return static_cast<std::uint32_t>(details_.first);
    }

    /// The fields of a header too long, of a table with too many fields, or
    /// of an incomplete table
    [[nodiscard]] std::size_t fields() const noexcept {
        auto fields = details_.kind == Kind::incomplete ? details_.second : details_.first;
        return static_cast<std::size_t>(fields);
    }

    /// The field that made a table too long
    [[nodiscard]] std::size_t index() const noexcept {
        return static_cast<std::size_t>(details_.first);
    }

    /// The length that a field made a table, the bytes of structs, or the
    /// length of a table that a writer didn't write as sized
    [[nodiscard]] std::uint64_t length() const noexcept {
        return details_.kind == Kind::not_whole_structs ? details_.first : details_.second;
    }

    /// The fields of an incomplete table that were set
    [[nodiscard]] std::size_t set() const noexcept {
        return static_cast<std::size_t>(details_.first);
    }

    /// The size of the structs whose bytes aren't whole
    [[nodiscard]] std::size_t size() const noexcept {
        return static_cast<std::size_t>(details_.second);
    }

    /// The bytes that a writer wrote of a table that it didn't write as
    /// sized
    [[nodiscard]] std::size_t written() const noexcept {
        return static_cast<std::size_t>(details_.first);
    }

    [[nodiscard]] const Details& details() const noexcept {
        return details_;
    }

private:
    static std::string message_of(const Details& details) {
        const auto first = std::to_string(details.first);
        const auto second = std::to_string(details.second);
        const auto offsets = std::string(name(details.offset_type));

        switch (details.kind) {
            case Kind::version_too_large:
                return "Version " + first + " doesn't fit in a " + offsets + " slot";
            case Kind::header_too_long:
                return "A header of " + first + " fields doesn't fit in " + offsets + " offsets";
            case Kind::too_many_fields:
                return "Can't add field " + std::to_string(details.first + 1) + " to a table of " + first;
            case Kind::too_long:
                return "Field " + std::to_string(details.first + 1) + " makes the table " + second +
                       " bytes, too long for " + offsets + " offsets";
            case Kind::incomplete:
                return "Only " + first + " of the table's " + second + " fields were set";
            case Kind::not_whole_structs:
                if (details.second == 0) {
                    return "A struct can't be 0 bytes";
                }
                return first + " bytes aren't a whole number of " + second + "-byte structs";
            case Kind::not_as_sized:
                break;
        }
        return "The fields wrote " + first + " of the table's " + second + " bytes";
    }

    Details details_;
};

namespace detail {

[[noreturn]] TAFLA_COLD inline void throw_out_of_range(std::size_t position, std::uint64_t byte_length,
                                                       std::size_t available) {
    throw DecodeError::out_of_range(position, byte_length, available);
}

/// Throws the error of `details`, which it takes by value, so that a writer
/// that throws one of its own doesn't give its address away
[[noreturn]] TAFLA_COLD inline void throw_build_error(BuildError::Details details) {
    throw BuildError(details);
}

}  // namespace detail
}  // namespace tafla

#endif
