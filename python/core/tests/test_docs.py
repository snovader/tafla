import doctest
import re
import unittest
from typing import override

import tafla


class MarkdownParser(doctest.DocTestParser):
    """Reads the examples of a Markdown file, whose code fences would otherwise
    read as the expected output of the example before them"""

    _FENCE = re.compile(r"^```.*$", re.MULTILINE)

    @override
    def parse(self, string: str, name: str = "<string>") -> list[str | doctest.Example]:
        return super().parse(self._FENCE.sub("", string), name)


def load_tests(
    loader: unittest.TestLoader, tests: unittest.TestSuite, pattern: str | None
) -> unittest.TestSuite:
    """Runs the examples in the package's docstring and its README as written"""
    tests.addTests(doctest.DocTestSuite(tafla))
    tests.addTests(doctest.DocFileSuite("../README.md", parser=MarkdownParser()))
    return tests


if __name__ == "__main__":
    unittest.main()
