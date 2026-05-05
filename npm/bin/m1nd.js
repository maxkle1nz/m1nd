#!/usr/bin/env node
"use strict";

const { main } = require("../lib/cli");

main(process.argv.slice(2)).catch((error) => {
  console.error(`m1nd: ${error.message}`);
  process.exitCode = 1;
});
