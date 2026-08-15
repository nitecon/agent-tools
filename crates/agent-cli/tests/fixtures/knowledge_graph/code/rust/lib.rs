mod worker;

pub trait Service {
    fn run(&self);
}

pub fn dispatch(service: &dyn Service) {
    service.run();
}
