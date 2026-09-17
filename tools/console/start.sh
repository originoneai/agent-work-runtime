#!/bin/sh
# 一键启动。不带参数就用当前目录作为项目目录。
#   ./start.sh                     # 把当前目录当作 AWR 项目
#   ./start.sh /path/to/project    # 指定项目目录
set -e
DIR=$(cd "$(dirname "$0")" && pwd)
PROJECT=${1:-$(pwd)}
exec node "$DIR/server.js" --project "$PROJECT"
