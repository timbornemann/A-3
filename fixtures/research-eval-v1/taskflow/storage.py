import os

class JsonStorage:
    def __init__(self, filepath):
        self.filepath = filepath

    def save_tasks(self, tasks):
        return ('json', self.filepath, tasks)

class SQLiteStorage:
    def __init__(self, filepath):
        self.filepath = filepath

    def save_tasks(self, tasks):
        return ('sqlite', self.filepath, tasks)

def create_storage(backend=None):
    selected = backend if backend is not None else os.environ.get('TASKFLOW_STORAGE', 'json')
    if selected == 'sqlite':
        return SQLiteStorage('tasks.db')
    if selected == 'json':
        return JsonStorage('tasks.json')
    raise ValueError('unsupported storage backend')
