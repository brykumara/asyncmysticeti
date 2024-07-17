def parse_dependencies(text):
    lines = text.strip().split('\n')
    dependencies = {}

    for line in lines:
        if 'refers to blocks' in line:
            block, refs = line.split(' refers to blocks ')
            refs_list = refs.split(', ')
            dependencies[block.split()[1]] = refs_list

    return dependencies

# The given text
text = """
block 2.1 refers to blocks 1.1, 1.2, 1.3
block 2.2 refers to blocks 1.1, 1.3, 1.4
block 2.3 refers to blocks 1.2, 1.3, 1.4
block 2.4 refers to blocks 1.1, 1.2, 1.3
block 3.1 refers to blocks 2.1, 2.3, 2.4
block 3.2 refers to blocks 2.1, 2.2, 2.3
block 3.3 refers to blocks 2.2, 2.3, 2.4
block 3.4 refers to blocks 2.1, 2.2, 2.3
block 4.1 refers to blocks 3.1, 3.2, 3.3
block 4.2 refers to blocks 3.1, 3.3, 3.4
block 4.3 refers to blocks 3.2, 3.3, 3.4
block 4.4 refers to blocks 3.1, 3.2, 3.3
"""

# Get the dependencies dictionary
dependencies = parse_dependencies(text)
print(dependencies)

