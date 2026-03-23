# Publishing `@relyo/sdk` to NPM

The Javascript SDK allows developers to interface with the Relyo network without handling the underlying cryptographic signatures manually. Follow these steps to push updates to the NPM registry.

## Step 1: Authentication

Authenticate via your terminal if you aren't already logged into the organization account.

```bash
npm login
```

## Step 2: Build Artifacts

The SDK is written in strict TypeScript. Compile the source into production-ready artifacts (`.js`, `.d.ts`).

```bash
cd ecosystem/relyo-js-sdk
npm install
npm run build
```

This standardizes the `/dist` directory for global distribution.

## Step 3: Publish

Publish the scoped package to the public registry. 

```bash
npm publish --access public
```

### Deployment Successful

The updated SDK can now be installed globally:

```bash
npm install @relyo/sdk
```
