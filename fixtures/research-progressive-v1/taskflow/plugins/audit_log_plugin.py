class AuditLogPlugin:
    def __init__(self):
        self.entries = []

    def on_task_created(self, task):
        self.entries.append(task)
