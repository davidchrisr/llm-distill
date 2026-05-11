FROM rust:1.95.0-trixie

WORKDIR /app

RUN apt update && apt install -y --no-install-recommends \
  bash \
  bat \
  ca-certificates \
  curl \
  eza \
  fd-find \
  git \
  jq \
  less \
  neovim \
  procps \
  ripgrep \
  tmux

RUN rustup component add rustfmt clippy

RUN groupadd --gid 10011 dev \
  && useradd --uid 10011 --gid dev --create-home --home-dir "/home/dev" --shell "/bin/bash" dev

USER dev:dev

COPY ".bash_extra" "/home/dev/.bash_extra"
RUN sed -i 's/^#force_color_prompt=yes$/force_color_prompt=yes/' "/home/dev/.bashrc"
RUN printf '\nif [ -f ~/.bash_extra ]; then\n    . ~/.bash_extra\nfi\n' >> "/home/dev/.bashrc"

CMD ["bash"]
