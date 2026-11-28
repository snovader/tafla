package harness

import (
	"fmt"
	"os/exec"
	"slices"
	"strings"
)

// Result is a benchmark's measurements: a workload's operation, and each
// variant's measurement of it, the first the one that the others are
// compared with.
type Result struct {
	Name         string
	Measurements []Measurement
}

// Report prints the fastest call of each benchmark, and its ratio to the
// first variant's, then the median call's distance from it, and the bytes
// and allocations of a call, as tables of Markdown.
func Report(results []Result) {
	columns := columnsOf(results)
	if len(columns) == 0 {
		return
	}

	fmt.Printf("Fastest call in µs, and its ratio to %s's:\n", columns[0])
	fmt.Println()
	table(results, columns, func(measurement, first Measurement) string {
		ratio := ""
		if measurement.Name != first.Name {
			ratio = fmt.Sprintf(" (%s×)", significant(measurement.Fastest/first.Fastest))
		}
		return micros(measurement.Fastest) + ratio
	})

	fmt.Println()
	fmt.Println("Median call's distance from the fastest:")
	fmt.Println()
	table(results, columns, func(measurement, _ Measurement) string {
		return fmt.Sprintf("%+.1f%%", 100*(measurement.Median/measurement.Fastest-1))
	})

	fmt.Println()
	fmt.Println("Bytes allocated per call, and the allocations that ask for them:")
	fmt.Println()
	table(results, columns, func(measurement, _ Measurement) string {
		return fmt.Sprintf("%s (%s)", Thousands(int(measurement.Bytes+0.5)), trimmed(measurement.Allocations))
	})
}

// columnsOf returns the variants' names, in their first order, of the
// benchmarks of every kind of variant
func columnsOf(results []Result) []string {
	var columns []string
	for _, result := range results {
		for _, measurement := range result.Measurements {
			if !slices.Contains(columns, measurement.Name) {
				columns = append(columns, measurement.Name)
			}
		}
	}

	return columns
}

func table(results []Result, columns []string, cell func(measurement, first Measurement) string) {
	fmt.Printf("| Benchmark | %s |\n", strings.Join(columns, " | "))
	fmt.Printf("| --- |%s\n", strings.Repeat(" ---: |", len(columns)))
	for _, result := range results {
		cells := make([]string, len(columns))
		for index, column := range columns {
			for _, measurement := range result.Measurements {
				if measurement.Name == column {
					cells[index] = cell(measurement, result.Measurements[0])
				}
			}
		}
		fmt.Printf("| %s | %s |\n", result.Name, strings.Join(cells, " | "))
	}
}

// Summary returns each variant's fastest call, on one line.
func Summary(measurements []Measurement) string {
	parts := make([]string, len(measurements))
	for index, measurement := range measurements {
		parts[index] = fmt.Sprintf("%s %s µs", measurement.Name, micros(measurement.Fastest))
	}

	return strings.Join(parts, ", ")
}

// micros returns a time in nanoseconds in µs, to three significant digits,
// or more integer digits
func micros(nanoseconds float64) string {
	return significant(nanoseconds / 1000)
}

func significant(value float64) string {
	switch {
	case value >= 100:
		return fmt.Sprintf("%.1f", value)
	case value >= 10:
		return fmt.Sprintf("%.2f", value)
	case value >= 1:
		return fmt.Sprintf("%.3f", value)
	case value >= 0.1:
		return fmt.Sprintf("%.3f", value)
	case value >= 0.01:
		return fmt.Sprintf("%.4f", value)
	default:
		return fmt.Sprintf("%.5f", value)
	}
}

func trimmed(value float64) string {
	return strings.TrimSuffix(strings.TrimRight(fmt.Sprintf("%.2f", value), "0"), ".")
}

// Thousands returns an integer with a comma between each group of three
// digits, as the benchmarks' names write their counts: 100,000.
func Thousands(value int) string {
	digits := fmt.Sprint(value)
	var grouped strings.Builder
	for index, digit := range digits {
		if index > 0 && (len(digits)-index)%3 == 0 {
			grouped.WriteByte(',')
		}
		grouped.WriteRune(digit)
	}

	return grouped.String()
}

// Uptime returns uptime's line, with the load averages.
func Uptime() string {
	output, err := exec.Command("uptime").Output()
	if err != nil {
		return err.Error() + "\n"
	}

	return string(output)
}
