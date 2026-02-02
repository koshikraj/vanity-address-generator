#!/bin/bash
# Run the vanity address generator and terminate the EC2 instance on completion.
# Usage: sudo bash run-and-terminate.sh
#
# To use as EC2 user data (runs on launch):
#   1. Base64-encode this script
#   2. Pass it as --user-data when launching the instance

set -e

PATTERN="${PATTERN:-c0ffee}"
SUFFIX="${SUFFIX:-93}"
WORKERS="${WORKERS:-$(nproc)}"
COUNT="${COUNT:-1}"

WORKDIR="/home/ubuntu/eth-vanity"
RESULT_FILE="/home/ubuntu/vanity-result.txt"

echo "=== Vanity Address Generator ==="
echo "Pattern: ${PATTERN} ... ${SUFFIX}"
echo "Workers: ${WORKERS}"
echo "Count:   ${COUNT}"
echo ""

# Build if needed
if [ ! -f "${WORKDIR}/target/release/eth_vanity" ]; then
    echo "Building..."
    cd "${WORKDIR}"
    cargo build --release
fi

# Run the generator and save output
echo "Starting search..."
"${WORKDIR}/target/release/eth_vanity" \
    -p "${PATTERN}" \
    -s "${SUFFIX}" \
    -w "${WORKERS}" \
    -n "${COUNT}" | tee "${RESULT_FILE}"

echo ""
echo "Results saved to ${RESULT_FILE}"
echo "Shutting down instance in 60 seconds... (Ctrl+C to cancel)"
sleep 60

# Terminate this instance
TOKEN=$(curl -s -X PUT "http://169.254.169.254/latest/api/token" \
    -H "X-aws-ec2-metadata-token-ttl-seconds: 30")
INSTANCE_ID=$(curl -s -H "X-aws-ec2-metadata-token: ${TOKEN}" \
    http://169.254.169.254/latest/meta-data/instance-id)

echo "Terminating instance ${INSTANCE_ID}..."
aws ec2 terminate-instances --instance-ids "${INSTANCE_ID}"
