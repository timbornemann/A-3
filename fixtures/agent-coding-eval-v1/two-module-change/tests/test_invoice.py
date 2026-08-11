from invoice import invoice_total


def test_invoice_applies_discount_before_formatting() -> None:
    assert invoice_total([1_000, 500], 10) == "$13.50"
