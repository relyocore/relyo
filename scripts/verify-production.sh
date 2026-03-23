#!/bin/bash
# Production System Verification Script

echo "=========================================="
echo "RELYO PRODUCTION SYSTEM VERIFICATION"
echo "=========================================="
echo ""

PASS_COUNT=0
FAIL_COUNT=0

check_item() {
  local name="$1"
  local cmd="$2"
  
  if eval "$cmd" > /dev/null 2>&1; then
    echo "✅ $name"
    ((PASS_COUNT++))
  else
    echo "❌ $name"
    ((FAIL_COUNT++))
  fi
}

echo "Checking Build & Binaries..."
check_item "Release binary exists (node)" "test -f target/release/relyo-node.exe"
check_item "Release binary exists (wallet)" "test -f target/release/relyo-wallet.exe"
check_item "Binary is executable (node)" "test -x target/release/relyo-node.exe"

echo ""
echo "Checking Production Directories..."
for i in {1..5}; do
  check_item "Node-$i directory exists" "test -d production-nodes/node-$i"
  check_item "Node-$i config exists" "test -f production-nodes/node-$i/relyo.toml"
  check_item "Node-$i has binary" "test -f production-nodes/node-$i/relyo-node.exe"
  check_item "Node-$i has data dir" "test -d production-nodes/node-$i/data"
  check_item "Node-$i startup script (.bat)" "test -f production-nodes/node-$i/start.bat"
  check_item "Node-$i startup script (.sh)" "test -f production-nodes/node-$i/start.sh"
done

echo ""
echo "Checking Documentation..."
check_item "Deployment guide exists" "test -f PRODUCTION_DEPLOYMENT.md"
check_item "Master launcher (.bat)" "test -f start-all-nodes.bat"
check_item "Master launcher (.sh)" "test -f start-all-nodes.sh"

echo ""
echo "Checking Configurations..."
for i in {1..5}; do
  CONFIG="production-nodes/node-$i/relyo.toml"
  check_item "Node-$i config has node_name" "grep -q 'node_name' $CONFIG"
  check_item "Node-$i config has data_dir" "grep -q 'data_dir' $CONFIG"
  check_item "Node-$i config has network section" "grep -q '\[network\]' $CONFIG"
done

echo ""
echo "Checking Port Allocation..."
PORTS_8="8001 8002 8003 8004 8005"
PORTS_9="9001 9002 9003 9004 9005 9701 9703 9705 9707 9709 9702 9704 9706 9708 9710"

# Check if ports are unique
for port in $PORTS_8 $PORTS_9; do
  count=$(grep -r "$port" production-nodes/*/relyo.toml | wc -l)
  if [ $count -eq 1 ]; then
    echo "✅ Port $port allocated uniquely"
    ((PASS_COUNT++))
  else
    echo "❌ Port $port conflict or missing"
    ((FAIL_COUNT++))
  fi
done

echo ""
echo "=========================================="
echo "VERIFICATION SUMMARY"
echo "=========================================="
echo "✅ Passed: $PASS_COUNT"
echo "❌ Failed: $FAIL_COUNT"
echo ""

if [ $FAIL_COUNT -eq 0 ]; then
  echo "🎉 ALL CHECKS PASSED!"
  echo "Production system is ready"
  exit 0
else
  echo "⚠️  Some checks failed - review above"
  exit 1
fi
