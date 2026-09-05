"""Offline regression fixture: a small factory must fit as a complete evidence unit."""

from .json_storage import JsonStorage
from .memory_storage import MemoryStorage
from .sqlite_storage import SqliteStorage


def get_storage(storage_type="sqlite", config=None):
    """Select an implementation without executing any storage operation here."""
    selected = storage_type.lower()
    if selected == "sqlite":
        db_path = config.db_path if config else "taskflow.db"
        return SqliteStorage(db_path=db_path)
    if selected == "json":
        json_path = config.json_path if config else "taskflow_db.json"
        return JsonStorage(filepath=json_path)
    if selected == "memory":
        return MemoryStorage()
    raise ValueError(f"Unsupported storage driver: {storage_type}")
