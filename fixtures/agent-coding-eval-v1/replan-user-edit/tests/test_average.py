from average import average


def test_average_preserves_fraction() -> None:
    assert average([2, 3]) == 2.5
