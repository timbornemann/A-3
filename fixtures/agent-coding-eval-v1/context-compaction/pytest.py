"""Tiny offline test runner for the deterministic coding-eval fixture."""

from pathlib import Path
import runpy
import sys


def main() -> int:
    tests = []
    for path in sorted(Path("tests").glob("test_*.py")):
        namespace = runpy.run_path(path)
        tests.extend(
            value
            for name, value in sorted(namespace.items())
            if name.startswith("test_") and callable(value)
        )
    failures = 0
    for test in tests:
        try:
            test()
        except AssertionError:
            failures += 1
    if failures:
        print(f"{failures} failed", file=sys.stderr)
        return 1
    print(f"{len(tests)} passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
