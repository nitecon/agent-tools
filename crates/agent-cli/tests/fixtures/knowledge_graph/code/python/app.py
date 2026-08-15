from worker import Worker


class Service:
    def run(self):
        raise NotImplementedError


def dispatch():
    Worker().run()
