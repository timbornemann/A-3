"""Independent public V2 oracle; never copied into the model's worktree.

Executed by the Rust test supervisor with python -I -B -c, not by the agent.
Only documented nonnegative monetary inputs and integer percentages are tested.
This is supplementary evaluation, not evidence injected into the agent ledger.
"""

import importlib
import sys


def check(case: str, root: str) -> None:
    sys.path.insert(0, root)
    if case == "small-local-bugfix":
        increment = importlib.import_module("increment").increment
        for value in [-10**18, -100, -2, -1, 0, 1, 2, 40, 41, 42, 100, 10**18]:
            assert increment(value) == value + 1
        return
    if case != "two-module-change":
        raise ValueError("unknown closed live coding case")

    pricing = importlib.import_module("pricing")
    percentages = [0, 1, 10, 33, 50, 99, 100]
    for cents in [0, 1, 99, 100, 101, 1_501, 1_234_567]:
        for percent in percentages:
            actual = pricing.discounted_total(cents, percent)
            assert type(actual) is int
            assert actual == cents * (100 - percent) // 100

    # Import after instrumentation so both `from pricing import ...` and
    # `import pricing` are valid. Prove the requested delegation, not its spelling.
    calls = []
    original = pricing.discounted_total

    def tracked(cents: int, percent: int) -> int:
        calls.append((cents, percent))
        return original(cents, percent)

    pricing.discounted_total = tracked
    invoice = importlib.import_module("invoice")
    for lines in [[], [0], [1], [101, 200, 7], [100_000, 9_999]]:
        for percent in percentages:
            before = list(lines)
            calls.clear()
            actual = invoice.invoice_total(lines, percent)
            total = sum(lines) * (100 - percent) // 100
            assert actual == f"${total // 100}.{total % 100:02d}"
            assert lines == before
            assert (sum(lines), percent) in calls


if __name__ == "__main__":
    check(sys.argv[1], sys.argv[2])
