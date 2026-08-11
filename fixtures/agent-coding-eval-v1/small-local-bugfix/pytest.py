"""Tiny offline test runner for the deterministic coding-eval fixture."""

from pathlib import Path
import runpy
import sys


def main() -> int:
    namespace = runpy.run_path(Path("tests") / "test_increment.py")
    test = namespace["test_increments_exactly_once"]
    try:
        test()
    except AssertionError:
        print("1 failed", file=sys.stderr)
        return 1
    print("1 passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
