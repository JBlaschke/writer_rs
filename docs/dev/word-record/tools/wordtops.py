"""Print the lines of a word.json written by wordq.py / wordrun.py.

usage: wordtops.py <word.json> [...]"""
import json
import sys


def main():
    for path in sys.argv[1:]:
        print(path.split("/")[-2])
        previous = None
        for line in json.load(open(path)):
            gap = ""
            if previous is not None and previous["page"] == line["page"]:
                gap = f"{line['top'] - previous['top']:7.2f}"
            base = ""
            if "baseline" in line:
                base = f"base={line['baseline']:8.2f}"
            print(f"  p{line['page']} top={line['top']:8.2f} {gap:>7} "
                  f"{base:<14} {line['style']:<14} {line['text'][:30]}")
            previous = line


if __name__ == "__main__":
    main()
