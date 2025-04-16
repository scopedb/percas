# Contributing

## Prerequisites

Percas uses [pre-commit](https://pre-commit.com/) to manage pre-commit hooks. This allows you to automatically format your code and run linters before committing changes.

To install pre-commit, you can use pip:

```shell
pip install pre-commit
```

And then, run the following command to install the pre-commit hooks:

```shell
pre-commit install
```

## Development tools

This project provides several targets under `cargo x` for accelerating development:

* `cargo x build`: Build the project.
* `cargo x test`: Run all tests.
* `cargo x lint`: Run all linters.
* `cargo x lint --fix`: Run all linters and try to fix any issues.
