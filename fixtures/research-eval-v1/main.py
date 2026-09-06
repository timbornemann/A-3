import argparse
from taskflow.manager import Manager

def build_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument('--backend', choices=['json', 'sqlite'])
    commands = parser.add_subparsers(dest='command', required=True)
    add = commands.add_parser('add')
    add.add_argument('project_id')
    add.add_argument('title')
    return parser

def main(argv=None):
    args = build_parser().parse_args(argv)
    manager = Manager(args.backend)
    if args.command == 'add':
        manager.add_task(args.project_id, args.title)
        return 0
    return 2

if __name__ == '__main__':
    raise SystemExit(main())
