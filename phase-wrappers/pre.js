const { env } = require('node:process');
env.GITHUB_RUST_ACTION_PHASE = 'pre';
const impl = require('./index.js');
