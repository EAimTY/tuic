# tuic-quinn

A thin layer on top of [quinn](https://github.com/quinn-rs/quinn) to provide functions for TUIC

[![Version](https://img.shields.io/crates/v/tuic-quinn.svg?style=flat)](https://crates.io/crates/tuic-quinn)
[![Documentation](https://img.shields.io/badge/docs-release-brightgreen.svg?style=flat)](https://docs.rs/tuic-quinn)
[![License](https://img.shields.io/crates/l/tuic-quinn.svg?style=flat)](https://github.com/EAimTY/tuic/blob/dev/LICENSE)

## Overview

This crate provides a wrapper [`Connection`](https://docs.rs/tuic-quinn/latest/tuic_quinn/struct.Connection.html) around [`quinn::Connection`](https://docs.rs/quinn/latest/quinn/struct.Connection.html). It can be used to perform TUIC operations.

Note that there is no state machine abstraction for the TUIC protocol flow in this crate. You need to implement it yourself.

## License

GNU General Public License v3.0
