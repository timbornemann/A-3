def invoice_total(line_cents: list[int], discount_percent: int) -> str:
    total = sum(line_cents)
    return f"${total // 100}.{total % 100:02d}"
