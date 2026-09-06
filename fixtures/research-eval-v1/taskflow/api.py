def get_task_response(manager, task_id):
    try:
        return {'task': manager.get_task(task_id)}, 200
    except KeyError:
        return {'error': 'task not found'}, 404

def dispatch(method, path, manager):
    if method == 'GET' and path.startswith('/tasks/'):
        return get_task_response(manager, int(path.rsplit('/', 1)[1]))
    return {'error': 'route not found'}, 404
