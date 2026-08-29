"""
Image Quality and Numerical Parity Metrics
Calculates PSNR, SSIM, and Maximum Absolute Difference (MAD) between two image tensors or arrays.
"""

import numpy as np


def compute_mad(a: np.ndarray, b: np.ndarray) -> float:
    """Compute Maximum Absolute Difference (L_inf norm)."""
    return float(np.max(np.abs(a - b)))


def compute_mse(a: np.ndarray, b: np.ndarray) -> float:
    """Compute Mean Squared Error."""
    return float(np.mean((a - b) ** 2))


def compute_psnr(a: np.ndarray, b: np.ndarray, data_range: float = 1.0) -> float:
    """Compute Peak Signal-to-Noise Ratio (PSNR) in dB."""
    mse = compute_mse(a, b)
    if mse == 0:
        return float("inf")
    return float(20.0 * np.log10(data_range / np.sqrt(mse)))


def compute_ssim(
    a: np.ndarray,
    b: np.ndarray,
    data_range: float = 1.0,
    k1: float = 0.01,
    k2: float = 0.03,
) -> float:
    """
    Compute Structural Similarity Index (SSIM) between two images.
    Expects shapes (H, W, C) or (C, H, W) normalized to [0, data_range].
    """
    c1 = (k1 * data_range) ** 2
    c2 = (k2 * data_range) ** 2

    mu_a = np.mean(a)
    mu_b = np.mean(b)

    sigma_a_sq = np.var(a)
    sigma_b_sq = np.var(b)
    sigma_ab = np.mean((a - mu_a) * (b - mu_b))

    numerator = (2 * mu_a * mu_b + c1) * (2 * sigma_ab + c2)
    denominator = (mu_a ** 2 + mu_b ** 2 + c1) * (sigma_a_sq + sigma_b_sq + c2)

    return float(numerator / denominator)
