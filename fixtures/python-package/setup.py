from setuptools import setup

setup(
    name="legacy-sample",
    install_requires=["requests>=2", "typing-extensions; python_version < '3.11'"],
    extras_require={"test": ["pytest>=8"]},
    packages=["sample"],
    entry_points={"console_scripts": ["legacy-sample = sample.cli:main"]},
)
