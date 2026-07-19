// Compile and exercise the consumer registry before its owner-side wiring is
// added to the library module table. Once wired, this remains an independent
// proof that the module has no hidden dependency on generic dispatch code.
#[path = "../src/action_consumers.rs"]
mod action_consumers;
