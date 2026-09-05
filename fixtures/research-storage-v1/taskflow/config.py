"""Fixture configuration: defaults, INI selection and an explicit environment override."""

import configparser
import os


class TaskFlowConfig:
    def __init__(self, config_file="config.ini"):
        self.storage_type = "sqlite"
        self.db_path = "taskflow.db"
        self.json_path = "taskflow_db.json"
        self.load(config_file)

    def load(self, config_file):
        parser = configparser.ConfigParser()
        parser.read(config_file, encoding="utf-8")
        if "Storage" in parser:
            section = parser["Storage"]
            self.storage_type = section.get("type", self.storage_type)
            self.db_path = section.get("sqlite_db", self.db_path)
            self.json_path = section.get("json_file", self.json_path)
        self.storage_type = os.getenv("TASKFLOW_STORAGE", self.storage_type)
