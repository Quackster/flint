mod lambda;
mod mono;
pub mod ownership;

pub use lambda::desugar_lambdas;
pub use mono::arity_msg;
pub use mono::monomorphize;
