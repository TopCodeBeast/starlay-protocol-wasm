import { SignAndSendSuccessResponse } from '@727-ventures/typechain-types'
import { ApiPromise } from '@polkadot/api'
import { WeightV2 } from '@polkadot/types/interfaces'
import { BN, BN_ONE } from '@polkadot/util'
import { ONE_ETHER } from './constants'

const WAIT_FINALIZED_SECONDS = 10000
const MAX_CALL_WEIGHT = new BN(900_000_000).isub(BN_ONE).mul(new BN(10))
const PROOFSIZE = new BN(1_000_000)

export const isTest = (): boolean => process.env.NODE_ENV === 'test'

export const percent = (val: number): BN => {
  return new BN(val).mul(ONE_ETHER).div(new BN(100))
}
export const waitForTx = async (
  result: SignAndSendSuccessResponse,
): Promise<void> => {
  if (isTest()) return

  while (!result.result.isFinalized) {
    await new Promise((resolve) => setTimeout(resolve, WAIT_FINALIZED_SECONDS))
  }
}
export const sendTxWithPreview = async <
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  C extends { tx: any; query: any; name: string },
  F extends keyof C['tx'],
  R extends ReturnType<C['tx'][F]>,
>(
  contract: C,
  fn: F,
  args: Parameters<C['tx'][F]>,
): Promise<R> => {
  try {
    const preview = await contract.query[fn](...args)
    if (preview.value.ok.err && Object.keys(preview.value.ok.err).length)
      throw new Error(preview.value.ok.err)
  } catch (e) {
    const calldata = `${contract.name}.${fn as string}(${JSON.stringify(args)})`
    throw new Error(`Failed to preview ${calldata}: ${JSON.stringify(e)}`)
  }
  const res = await contract.tx[fn](...args)
  await waitForTx(res)
  const calldata = `${contract.name}.${fn as string}(${JSON.stringify(
    args.slice(0, -1),
  )})`
  console.log(`Transaction succeeded: ${calldata}`)
  return res
}

export const defaultOption = (
  api: ApiPromise,
): {
  storageDepositLimit: BN
  gasLimit: WeightV2
} => {
  return {
    storageDepositLimit: new BN(10).pow(new BN(18)),
    gasLimit: getGasLimit(api),
  }
}

export const getGasLimit = (
  api: ApiPromise,
  refTime?: BN | number,
  proofSize?: BN | number,
): WeightV2 => {
  refTime = refTime || MAX_CALL_WEIGHT
  proofSize = proofSize || PROOFSIZE
  return api.registry.createType('WeightV2', {
    refTime: refTime,
    proofSize: proofSize,
  })
}

export const hexToUtf8 = (hexArray: number[]): string =>
  Buffer.from(hexArray.toString().replace('0x', ''), 'hex').toString('utf-8')
