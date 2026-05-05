const { env } = require('node:process');
env.GITHUB_RUST_ACTION_PHASE = 'main';
const impl = require('./index.js');
