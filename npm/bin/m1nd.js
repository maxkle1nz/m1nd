#!/usr/bin/env node
"use strict";

const { main } = require("../lib/cli");

const INTERRUPTED_EXIT_CODES = Object.freeze({
  SIGHUP: 129,
  SIGINT: 130,
  SIGTERM: 143,
});

function interruptedExitCode(error) {
  if (!error || error.code !== "M1ND_AGENT_INTERRUPTED") return null;
  const expected = INTERRUPTED_EXIT_CODES[error.signal];
  return Number.isInteger(expected) && error.exitCode === expected ? expected : null;
}

main(process.argv.slice(2)).catch((error) => {
  console.error(`m1nd: ${error.message}`);
  process.exitCode = interruptedExitCode(error) || 1;
});
