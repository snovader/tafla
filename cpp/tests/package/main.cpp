// Writes and reads a table with the installed headers

#include <tafla/tafla.hpp>

int main() {
    tafla::TableBuilder builder(1, tafla::TableLayout());
    builder.set_string("installed");
    const auto bytes = builder.to_bytes();

    const tafla::TableReader reader(bytes, tafla::TableLayout());
    return reader.get_string(reader.root_offset(0)) == "installed" ? 0 : 1;
}
