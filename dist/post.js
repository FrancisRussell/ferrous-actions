const { env } = require('node:process');
env.GITHUB_RUST_ACTION_PHASE = 'post';
const impl = require('./index.js');
