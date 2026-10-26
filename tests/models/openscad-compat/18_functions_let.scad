// Purpose: functions, let, list expressions, recursion and function literals drive a staircase.
function sum(v, i = 0) = i >= len(v) ? 0 : v[i] + sum(v, i + 1);
function fib(n) = n < 2 ? n : fib(n - 1) + fib(n - 2);
function cumsum(v) = [for (i = [0 : len(v) - 1]) sum([for (j = [0 : i]) v[j]])];
steps = [for (i = [1 : 6]) fib(i) + 2];
heights = cumsum(steps);
dbl = function (x) x * 2;
echo(steps = steps, heights = heights, total = sum(steps));
echo(let(a = 3, b = a * a) [a, b], dbl(21), [for (s = steps) if (s % 2 == 0) s]);
for (i = [0 : len(steps) - 1])
    let(x = i * 8, h = heights[i])
        translate([x, 0, 0]) cube([8, 10, h]);
