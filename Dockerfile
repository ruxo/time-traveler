# Time Traveler's image (design section 8). Build context: the repository root. No BuildKit syntax: Erawan's
# builder cannot parse cache mounts or COPY --link, so the dependencies compile on every build.

# ---- build ----------------------------------------------------------------------------------
FROM rust:1-slim-bookworm AS build
WORKDIR /src

# The whole workspace: Cargo refuses a workspace whose member manifest is missing, so the migrate tool's sources
# come along although the image does not ship it. `public/` is embedded into the binary by rust-embed.
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY src/traveler ./src/traveler
COPY src/traveler-migrate ./src/traveler-migrate
COPY public ./public

RUN cargo build --release --locked --package traveler \
 && cp target/release/traveler /traveler

# ---- runtime --------------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /traveler /app/traveler
# Defaults only; every environment-specific value comes from the environment (appsettings.toml says which).
COPY src/traveler/appsettings.toml /app/appsettings.toml

EXPOSE 8080

# Exec form: the binary is PID 1 and SIGTERM reaches its shutdown handler.
ENTRYPOINT ["/app/traveler"]
