# parse_options: {"target-version": "3.15"}
{**x if c else y for x in xs}
{**x or y for x in xs}
{**x < y for x in xs}
{**lambda: x for x in xs}
(*x if c else y for x in xs)
(*x or y for x in xs)
(*x < y for x in xs)
(*lambda: x for x in xs)
f(*x if c else y for x in xs)
f(*x if c else y)
(*(*x or y for x in z) for z in zs)
async def unpack(xs):
    return {**x if c else y async for x in xs}
async def iterate(xs):
    return (*x if c else y async for x in xs)
