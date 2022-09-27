FROM rust:1.63.0-slim as rust_builder

RUN USER=root cargo new --bin word-of-the-day-bot
WORKDIR /word-of-the-day-bot
COPY ./Cargo.toml ./Cargo.toml
RUN cargo build --release
RUN rm src/*.rs

COPY ./src ./src
COPY ./Cargo.lock ./Cargo.lock

RUN rm ./target/release/deps/word_of_the_day_bot*
RUN cargo build --release

FROM debian:buster-slim
ARG APP=/usr/src/app

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata 

EXPOSE 3030

ENV TZ=Etc/UTC \
    APP_USER=appuser

RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY --from=rust_builder /word-of-the-day-bot/target/release/word-of-the-day-bot ${APP}/word-of-the-day-bot

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

ENV RUST_BACKTRACE=1

CMD ["./word-of-the-day-bot"]
