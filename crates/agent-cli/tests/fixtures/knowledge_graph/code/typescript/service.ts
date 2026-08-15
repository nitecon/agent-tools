export interface Service {
  run(): void;
}

export class Worker implements Service {
  run(): void {}
}

export function dispatch(service: Service): void {
  service.run();
}
