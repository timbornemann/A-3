class PluginDispatcher:
    def __init__(self):
        self.plugins = []

    def dispatch(self, event, task):
        for plugin in self.plugins:
            plugin.on_task_created(task)
