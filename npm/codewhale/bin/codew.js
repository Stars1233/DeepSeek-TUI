#!/usr/bin/env node

const { run } = require("../scripts/run");

run("codew").catch((error) => {
  console.error("Failed to start codew:", error.message);
  process.exit(1);
});
