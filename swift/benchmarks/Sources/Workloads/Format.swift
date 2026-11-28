/// `value` in decimal digits, grouped in thousands with commas
public func thousands(_ value: Int) -> String {
    let digits = String(value.magnitude)
    var grouped = value < 0 ? "-" : ""
    for (index, digit) in digits.enumerated() {
        if index > 0, (digits.count - index) % 3 == 0 {
            grouped.append(",")
        }
        grouped.append(digit)
    }

    return grouped
}
