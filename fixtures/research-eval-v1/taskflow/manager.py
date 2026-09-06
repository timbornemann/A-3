from taskflow.storage import create_storage
from taskflow.plugins import PluginManager

class Manager:
    def __init__(self, backend=None):
        self.storage = create_storage(backend)
        self.plugins = PluginManager()
        self.projects = {'inbox': []}
        self.tasks = []

    def add_task(self, project_id, title):
        if project_id not in self.projects:
            raise ValueError('unknown project')
        task = {'project_id': project_id, 'title': title}
        self.tasks.append(task)
        self.storage.save_tasks(self.tasks)
        self.plugins.trigger_task_created(task)
        return task

    def get_task(self, task_id):
        if task_id < 0 or task_id >= len(self.tasks):
            raise KeyError(task_id)
        return self.tasks[task_id]
