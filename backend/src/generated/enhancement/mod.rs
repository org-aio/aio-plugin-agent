//! 模型只生成候选源码；运行时独立验证契约、授权并提交版本。

pub mod configuration;
pub mod controller;
pub mod model;
pub mod service;
mod service_impl;
mod util;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod protocol_tests;
