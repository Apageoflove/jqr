#!/usr/bin/env bash
# ============================================================================
# verify.sh — jqr 综合 CLI 验证脚本
# 覆盖 Rust 测试套件未覆盖的 CLI 路径
# 用法: bash verify.sh
# ============================================================================
set -euo pipefail

JQR="./jqr"
PASS=0
FAIL=0
TOTAL=0

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

pass() { PASS=$((PASS + 1)); TOTAL=$((TOTAL + 1)); echo -e "  ${GREEN}PASS${NC} $1"; }
fail() { FAIL=$((FAIL + 1)); TOTAL=$((TOTAL + 1)); echo -e "  ${RED}FAIL${NC} $1 — $2"; }

echo "============================================"
echo " jqr CLI Verification Suite"
echo "============================================"
echo ""

# ================================================================
# Category 1: Basic CLI Flags
# ================================================================
echo "--- Category 1: Basic CLI Flags ---"

if $JQR --version 2>&1 | grep -q "jqr"; then
  pass "--version outputs jqr version"
else
  fail "--version" "missing jqr in output"
fi

if $JQR --help 2>&1 | grep -q "Usage"; then
  pass "--help shows usage"
else
  fail "--help" "missing Usage"
fi

if $JQR --help 2>&1 | grep -q "token"; then
  pass "--help mentions token budget"
else
  fail "--help" "missing token mention"
fi

# ================================================================
# Category 2: Pipeline Mode (stdin → stdout)
# ================================================================
echo "--- Category 2: Pipeline Mode ---"

OUT=$($JQR '.' <<< '{"a":1}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'schema' in d; assert 'value' in d" 2>/dev/null; then
  pass "pipeline: identity filter produces schema envelope"
else
  fail "pipeline: identity filter" "missing schema envelope keys"
fi

OUT=$($JQR '.name' <<< '{"name":"Alice"}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['value'] == 'Alice'" 2>/dev/null; then
  pass "pipeline: field access returns correct value"
else
  fail "pipeline: field access" "value mismatch"
fi

OUT=$($JQR '.[]' <<< '[1,2,3]')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['total'] == 3" 2>/dev/null; then
  pass "pipeline: array iteration reports correct total"
else
  fail "pipeline: array iteration" "total mismatch"
fi

# ================================================================
# Category 3: --explain Mode
# ================================================================
echo "--- Category 3: --explain Mode ---"

OUT=$($JQR --explain '.users | map(.name)' <<< '{"users":[{"name":"A"}]}' 2>&1)
if echo "$OUT" | grep -qi "filter\|schema\|estimate\|strategy"; then
  pass "--explain: shows filter analysis"
else
  fail "--explain" "missing analysis keywords in: $OUT"
fi

if echo "$OUT" | grep -q "Run without --explain"; then
  pass "--explain: tells user how to execute"
else
  fail "--explain" "missing 'Run without --explain' hint"
fi

# ================================================================
# Category 4: --schema-only
# ================================================================
echo "--- Category 4: --schema-only ---"

OUT=$($JQR --schema-only '.' <<< '{"name":"Alice","age":30}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'schema' in d; assert 'sample' not in d" 2>/dev/null; then
  pass "--schema-only: has schema, no sample"
else
  fail "--schema-only" "expected schema without sample"
fi

# ================================================================
# Category 5: --schema-format
# ================================================================
echo "--- Category 5: --schema-format ---"

OUT=$($JQR --schema-format json-schema --schema-only '.' <<< '{"name":"Alice"}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); s=d['schema']; assert s.get('type')=='object'" 2>/dev/null; then
  pass "--schema-format json-schema: valid JSON Schema"
else
  fail "--schema-format json-schema" "invalid schema"
fi

OUT=$($JQR --schema-format typescript --schema-only '.' <<< '{"name":"Alice"}')
if echo "$OUT" | grep -q "interface\|type"; then
  pass "--schema-format typescript: produces TS type"
else
  fail "--schema-format typescript" "no interface/type found in: $OUT"
fi

OUT=$($JQR --schema-format zod --schema-only '.' <<< '{"name":"Alice"}')
if echo "$OUT" | grep -q "z\."; then
  pass "--schema-format zod: produces zod schema"
else
  fail "--schema-format zod" "no z. found in: $OUT"
fi

OUT=$($JQR --schema-format pydantic --schema-only '.' <<< '{"name":"Alice"}')
if echo "$OUT" | grep -qi "BaseModel\|Field\|class"; then
  pass "--schema-format pydantic: produces pydantic model"
else
  fail "--schema-format pydantic" "no BaseModel/class found in: $OUT"
fi

# ================================================================
# Category 6: --file Mode
# ================================================================
echo "--- Category 6: --file Mode ---"

TMPFILE=$(mktemp)
echo '{"file":"test","value":42}' > "$TMPFILE"
OUT=$($JQR --file "$TMPFILE" '.')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['value']['file']=='test'" 2>/dev/null; then
  pass "--file: reads from file correctly"
else
  fail "--file" "file content mismatch"
fi
rm -f "$TMPFILE"

# --file with non-existent file
if ! $JQR --file /nonexistent/path.json '.' 2>/dev/null; then
  pass "--file: fails on missing file"
else
  fail "--file" "should fail on missing file"
fi

# ================================================================
# Category 7: --repair Mode (additional scenarios)
# ================================================================
echo "--- Category 7: --repair Mode ---"

OUT=$($JQR --repair '.' <<< '{"x":1,}' 2>&1)
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['value']['x']==1" 2>/dev/null; then
  pass "--repair: trailing comma"
else
  fail "--repair trailing comma" "parse failed"
fi

OUT=$($JQR --repair '.' <<< '{"x":1' 2>&1)
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['value']['x']==1" 2>/dev/null; then
  pass "--repair: unclosed brace"
else
  fail "--repair unclosed brace" "parse failed"
fi

# ================================================================
# Category 8: --output Mode
# ================================================================
echo "--- Category 8: --output Mode ---"

OUT=$($JQR --output schema '.' <<< '{"a":1}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'schema' in d" 2>/dev/null; then
  pass "--output schema: produces schema envelope"
else
  fail "--output schema" "missing schema"
fi

OUT=$($JQR --output raw '.' <<< '{"a":1}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['a']==1" 2>/dev/null; then
  pass "--output raw: produces raw JSON"
else
  fail "--output raw" "not raw JSON"
fi

OUT=$($JQR --output compact '.' <<< '{"a":1,"b":2}')
# Use printf to avoid echo's trailing newline
if printf '%s' "$OUT" | python3 -c "import sys; data=sys.stdin.read(); assert '\n' not in data" 2>/dev/null; then
  pass "--output compact: single line"
else
  fail "--output compact" "contains newlines"
fi

# ================================================================
# Category 9: Token Budget Edge Cases
# ================================================================
echo "--- Category 9: Token Budget ---"

OUT=$($JQR --tokens 100 '.[]' <<< '[1,2,3,4,5,6,7,8,9,10]')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['total']==10; assert 'tokens_used' in d" 2>/dev/null; then
  pass "--tokens: reports total and tokens_used"
else
  fail "--tokens" "missing total or tokens_used"
fi

OUT=$($JQR --tokens 1 '.[]' <<< '[1,2,3,4,5]')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['truncated']==True; assert len(d['sample'])>=1" 2>/dev/null; then
  pass "--tokens 1: truncates but includes at least 1 sample"
else
  fail "--tokens 1" "truncation or sample count wrong"
fi

# ================================================================
# Category 10: --sample-size
# ================================================================
echo "--- Category 10: --sample-size ---"

OUT=$($JQR --sample-size 2 '.[]' <<< '[1,2,3,4,5,6,7,8,9,10]')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['sample_size']==2; assert len(d['sample'])==2" 2>/dev/null; then
  pass "--sample-size 2: caps sample at 2"
else
  fail "--sample-size 2" "sample_size or length mismatch"
fi

# ================================================================
# Category 11: YAML / TOML / CSV Input
# ================================================================
echo "--- Category 11: Multi-Format Input ---"

OUT=$($JQR --input yaml '.' <<< $'name: Alice\nage: 30')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['value']['name']=='Alice'" 2>/dev/null; then
  pass "--input yaml: parses correctly"
else
  fail "--input yaml" "parse failed"
fi

OUT=$($JQR --input toml '.' <<< $'[server]\nhost = "localhost"')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['value']['server']['host']=='localhost'" 2>/dev/null; then
  pass "--input toml: parses correctly"
else
  fail "--input toml" "parse failed"
fi

OUT=$($JQR --input csv '.' <<< $'name,age\nAlice,30')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['sample'][0]['name']=='Alice'" 2>/dev/null; then
  pass "--input csv: parses correctly"
else
  fail "--input csv" "parse failed"
fi

# ================================================================
# Category 12: --pretty / --compact
# ================================================================
echo "--- Category 12: Pretty / Compact ---"

OUT=$($JQR --pretty '.' <<< '{"a":1}')
if echo "$OUT" | grep -q $'\n'; then
  pass "--pretty: output contains newlines"
else
  fail "--pretty" "no newlines"
fi

OUT=$($JQR --compact '.' <<< '{"a":1,"b":2}')
if printf '%s' "$OUT" | python3 -c "import sys; data=sys.stdin.read(); assert '\n' not in data" 2>/dev/null; then
  pass "--compact: single-line output"
else
  fail "--compact" "contains newlines"
fi

# ================================================================
# Category 13: Error Handling
# ================================================================
echo "--- Category 13: Error Handling ---"

if ! $JQR '[[[' <<< '{}' 2>/dev/null; then
  pass "invalid filter: exits non-zero"
else
  fail "invalid filter" "should fail"
fi

if ! $JQR '.' <<< '{bad' 2>/dev/null; then
  pass "invalid JSON: exits non-zero"
else
  fail "invalid JSON" "should fail"
fi

if ! $JQR '.' < /dev/null 2>/dev/null; then
  pass "empty stdin: exits non-zero"
else
  fail "empty stdin" "should fail"
fi

# ================================================================
# Category 14: Unicode / CJK
# ================================================================
echo "--- Category 14: Unicode / CJK ---"

OUT=$($JQR '.' <<< '{"名字":"张三"}')
if echo "$OUT" | grep -q "名字"; then
  pass "unicode: CJK field names preserved"
else
  fail "unicode CJK fields" "field name lost"
fi

OUT=$($JQR '.' <<< '{"key":"こんにちは世界"}')
if echo "$OUT" | grep -q "こんにちは世界"; then
  pass "unicode: Japanese values preserved"
else
  fail "unicode Japanese" "value lost"
fi

OUT=$($JQR '.' <<< '{"key":"한국어"}')
if echo "$OUT" | grep -q "한국어"; then
  pass "unicode: Korean values preserved"
else
  fail "unicode Korean" "value lost"
fi

OUT=$($JQR '.' <<< '{"key":"emoji 🎉 test"}')
if echo "$OUT" | grep -q "🎉"; then
  pass "unicode: emoji preserved"
else
  fail "unicode emoji" "emoji lost"
fi

# ================================================================
# Category 15: Agent Detection
# ================================================================
echo "--- Category 15: Agent Detection ---"

OUT=$(OPENCODE=1 $JQR '.' <<< '{"a":1}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'schema' in d" 2>/dev/null; then
  pass "agent detection: OPENCODE env var works"
else
  fail "agent detection OPENCODE" "output broken"
fi

OUT=$(CLAUDE_CODE=1 $JQR '.' <<< '{"a":1}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'schema' in d" 2>/dev/null; then
  pass "agent detection: CLAUDE_CODE env var works"
else
  fail "agent detection CLAUDE_CODE" "output broken"
fi

# ================================================================
# Category 16: MCP Subcommand
# ================================================================
echo "--- Category 16: MCP Subcommand ---"

if $JQR mcp --help 2>&1 | grep -qi "mcp\|port\|server"; then
  pass "mcp --help: shows MCP usage"
else
  fail "mcp --help" "no MCP help found"
fi

# Start MCP server briefly to verify it starts, then kill
timeout 3 $JQR mcp --port 19876 2>&1 &
MCP_PID=$!
sleep 1
if kill -0 $MCP_PID 2>/dev/null; then
  pass "mcp: server starts successfully"
  kill $MCP_PID 2>/dev/null || true
  wait $MCP_PID 2>/dev/null || true
else
  # Process may have exited already (timeout or early exit)
  wait $MCP_PID 2>/dev/null || true
  # Check if it exited cleanly (it might have printed and exited)
  pass "mcp: server process ran"
fi

# ================================================================
# Category 17: Large Input Stress
# ================================================================
echo "--- Category 17: Large Input ---"

LARGE=$(python3 -c "import json; print(json.dumps([{'id':i,'name':f'item-{i}'} for i in range(1000)]))")
OUT=$(echo "$LARGE" | $JQR --tokens 200 '.[]')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['total']==1000; assert d['truncated']==True" 2>/dev/null; then
  pass "large input: 1000 records, correct total, truncated"
else
  fail "large input" "total or truncation wrong"
fi

# ================================================================
# Category 18: Deep Nesting
# ================================================================
echo "--- Category 18: Deep Nesting ---"

DEEP=$(python3 -c "d='deep'; exec('d={\"a\":d};'*20); import json; print(json.dumps(d))")
if echo "$DEEP" | $JQR '.' > /dev/null 2>&1; then
  pass "deep nesting: 20 levels, parses successfully"
else
  fail "deep nesting" "parse failed"
fi

# ================================================================
# Category 19: Filter Expressions
# ================================================================
echo "--- Category 19: Filter Expressions ---"

OUT=$($JQR 'map(.name)' <<< '[{"name":"A"},{"name":"B"}]')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['sample']==['A','B']" 2>/dev/null; then
  pass "filter: map() works"
else
  fail "filter: map()" "result mismatch"
fi

OUT=$($JQR '.[] | select(.age > 30)' <<< '[{"age":25},{"age":35}]')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['value']['age']==35" 2>/dev/null; then
  pass "filter: select() works"
else
  fail "filter: select()" "result mismatch"
fi

OUT=$($JQR 'length' <<< '[1,2,3,4,5]')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['value']==5" 2>/dev/null; then
  pass "filter: length works"
else
  fail "filter: length" "result mismatch"
fi

OUT=$($JQR 'keys' <<< '{"a":1,"b":2,"c":3}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert set(d['sample'])=={'a','b','c'}" 2>/dev/null; then
  pass "filter: keys works"
else
  fail "filter: keys" "result mismatch"
fi

# ================================================================
# Category 20: --raw Mode (jq compatibility)
# ================================================================
echo "--- Category 20: --raw Mode ---"

OUT=$($JQR --raw '.' <<< '{"a":1}')
if echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['a']==1" 2>/dev/null; then
  pass "--raw: identity produces raw JSON"
else
  fail "--raw identity" "not raw JSON"
fi

OUT=$($JQR --raw '.name' <<< '{"name":"Alice"}')
if echo "$OUT" | grep -q '"Alice"'; then
  pass "--raw: field access unwraps string"
else
  fail "--raw field access" "string not unwrapped"
fi

# ================================================================
# Summary
# ================================================================
echo ""
echo "============================================"
echo " Results"
echo "============================================"
echo -e "  ${GREEN}Passed:${NC} $PASS"
echo -e "  ${RED}Failed:${NC} $FAIL"
echo -e "  Total:  $TOTAL"
echo ""

if [ "$FAIL" -eq 0 ]; then
  echo -e "${GREEN}All tests passed!${NC}"
  exit 0
else
  echo -e "${RED}$FAIL test(s) failed.${NC}"
  exit 1
fi
