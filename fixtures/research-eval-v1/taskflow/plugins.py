import os

class AuditLogPlugin:
    def __init__(self, log_filepath='audit_log.txt'):
        self.log_filepath = os.path.abspath(log_filepath)

    def on_task_created(self, task):
        self._log('TASK_CREATED', task)

    def _log(self, event, task):
        with open(self.log_filepath, 'a', encoding='utf-8') as output:
            output.write(f'{event}: {task}\n')

class PluginManager:
    def __init__(self):
        self.plugins = [AuditLogPlugin()]

    def trigger_task_created(self, task):
        for plugin in self.plugins:
            plugin.on_task_created(task)
