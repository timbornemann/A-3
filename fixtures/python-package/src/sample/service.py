"""Service implementation used by the Python adapter fixture."""

from __future__ import annotations

import json
import pathlib as paths
from collections.abc import Callable as Callback

from .base import BaseService
from .helpers import helper

__all__ = ["Service", "build_service"]


class Service(BaseService):
    """Decode payloads through an injected callback."""

    def __init__(self, callback: Callback[[str], str]) -> None:
        self._callback = callback

    def run(self, payload: str) -> object:
        """Decode one payload."""
        return json.loads(self._callback(payload))

    def _helper(self) -> str:
        return helper()

    def __private(self) -> paths.Path:
        return paths.Path(".")


async def build_service(callback: Callback[[str], str]) -> Service:
    service = Service(callback)
    await notify(service)
    return service


def _internal_task() -> None:
    helper()
