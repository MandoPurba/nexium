FROM node:20-alpine AS builder

WORKDIR /app

# Build-time env vars — baked into the Next.js JS bundle
ARG NEXT_PUBLIC_API_URL=http://localhost:8084
ARG NEXT_PUBLIC_WS_URL=ws://localhost:8084
ENV NEXT_PUBLIC_API_URL=$NEXT_PUBLIC_API_URL
ENV NEXT_PUBLIC_WS_URL=$NEXT_PUBLIC_WS_URL

COPY web/package.json ./
RUN npm install
COPY web/ .
RUN npm run build

FROM node:20-alpine AS runner

WORKDIR /app
ENV NODE_ENV=production
COPY --from=builder /app/.next/standalone ./
COPY --from=builder /app/.next/static ./.next/static
COPY --from=builder /app/public ./public

EXPOSE 3000
CMD ["node", "server.js"]
