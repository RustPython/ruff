# parse_options: {"target-version": "3.15"}
match x:
    case {**rest, 1: value}:
        pass
