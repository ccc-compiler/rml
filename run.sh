for f in fib lists tree ownership; do
  echo "== $f"; ./rmlc examples/$f.rml --run 2>&1 | tail -15; 
done; 
echo "== err"; 