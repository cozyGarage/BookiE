ELAPSED_SEPARATOR = " · "


def paginator_name_matches(name, expected):
    return name == expected or name.startswith(expected + ELAPSED_SEPARATOR)
