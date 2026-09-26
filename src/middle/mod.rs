mod lambda;
mod mono;

pub use lambda::desugar_lambdas;
pub use mono::arity_msg;
pub use mono::monomorphize;
