import unittest

import pytest

from sample.service import Service, build_service


@pytest.fixture
def service() -> Service:
    return Service(str)


@pytest.mark.parametrize("payload", ["{}"])
def test_build(payload: str) -> None:
    build_service(str)


class ServiceTests(unittest.TestCase):
    def test_run(self) -> None:
        self.assertEqual(service().run("{}"), {})


class TestPytestStyle:
    def test_method(self, service: Service) -> None:
        assert service.run("{}") == {}
