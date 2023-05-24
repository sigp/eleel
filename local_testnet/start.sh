#!/usr/bin/env bash

set -Eeuo pipefail

# Create fresh directory containing LH scripts and overrides
rm -rf scripts
mkdir scripts
cp -rf lighthouse/scripts/local_testnet/* scripts/
cp -rf overrides/* scripts/

cd scripts

# Load variables
source ./vars.env

# Lighthouse binary to run with Eleel
ROGUE_LH="${ROGUE_LH:-lighthouse}"

# Variables copied from the Lighthouse scripts (make sure these match)
PID_FILE=$TESTNET_DIR/PIDS.pid
LOG_DIR=$TESTNET_DIR
BN_udp_tcp_base=9000
BN_http_port_base=8000
EL_base_network=7000
EL_base_http=6000
EL_base_auth_http=5000
genesis_file=genesis.json

# Execute the command with logs saved to a file.
#
# First parameter is log file name
# Second parameter is executable name
# Remaining parameters are passed to executable
execute_command() {
    LOG_NAME=$1
    EX_NAME=$2
    shift
    shift
    CMD="$EX_NAME $@ >> $LOG_DIR/$LOG_NAME 2>&1"
    echo "executing: $CMD"
    echo "$CMD" > "$LOG_DIR/$LOG_NAME"
    eval "$CMD &"
}

# Execute the command with logs saved to a file
# and is PID is saved to $PID_FILE.
#
# First parameter is log file name
# Second parameter is executable name
# Remaining parameters are passed to executable
execute_command_add_PID() {
    execute_command $@
    echo "$!" >> $PID_FILE
}

# Start all stock nodes including N validator clients and N - 1 beacon nodes
./start_local_testnet.sh $genesis_file

# Start the rogue Geth instance
el=$((BN_COUNT + 1))
bn=$el

geth_port=$((EL_base_auth_http + $el))
eleel_port=$(($geth_port + 1))

touch $LOG_DIR/rogue_geth.log
execute_command_add_PID rogue_geth.log ./geth.sh $DATADIR/rogue_geth_datadir $((EL_base_network + $el)) $((EL_base_http + $el)) $geth_port $genesis_file

# Wait for Geth to start so eleel can use its JWT
sleep 10

# Start eleel
secret=$DATADIR/rogue_geth_datadir/geth/jwtsecret

touch $LOG_DIR/eleel.log
execute_command_add_PID eleel.log eleel \
    --ee-url http://localhost:$geth_port \
    --ee-jwt-secret $secret \
    --listen-port $eleel_port

# Start the rogue Lighthouse instance connected to eleel
touch $LOG_DIR/rogue_lh.log
ROGUE_LH=$ROGUE_LH execute_command_add_PID rogue_lh.log ./rogue_lighthouse.sh $DATADIR/rogue_lh $((BN_udp_tcp_base + $bn)) $((BN_http_port_base + $bn)) http://localhost:$eleel_port $secret

echo "Up!"
