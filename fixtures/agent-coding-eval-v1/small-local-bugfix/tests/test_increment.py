from increment import increment


def test_increments_exactly_once() -> None:
    assert increment(41) == 42
