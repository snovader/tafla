// Writes and reads a table with the installed headers

#include <tafla/tafla.hpp>

int main() {
    tafla::TableBuilder builder(1, tafla::TableLayout());
    builder.set_string("installed");
    const auto bytes = builder.to_bytes();

    const tafla::ByteBuffer buffer(bytes, tafla::TableLayout());
    return buffer.get_string(buffer.root_offset(0)) == "installed" ? 0 : 1;
}
