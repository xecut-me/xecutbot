#!/usr/bin/env bash

set -euxo pipefail

git pull
sqlx migrate run
./build.sh
