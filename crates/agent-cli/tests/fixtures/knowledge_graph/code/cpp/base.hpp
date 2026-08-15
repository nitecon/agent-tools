struct Service {
    virtual void run() = 0;
};

struct Worker : Service {
    void run() override;
};
