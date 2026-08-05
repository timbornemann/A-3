from .base import BaseService


class Service(BaseService):
    def run(self) -> int:
        return helper()


def helper() -> int:
    return 1


def main() -> int:
    return Service().run()


def dynamic(factory):
    return factory().run()
