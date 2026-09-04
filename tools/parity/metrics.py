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
    Expects shapes (H, W, C) or (H, W) normalized to [0, data_range].
    """
    try:
        from skimage.metrics import structural_similarity
        channel_axis = -1 if a.ndim == 3 else None
        return float(
            structural_similarity(
                a, b, data_range=data_range, channel_axis=channel_axis, K1=k1, K2=k2
            )
        )
    except ImportError:
        try:
            import scipy.ndimage as ndimage

            c1 = (k1 * data_range) ** 2
            c2 = (k2 * data_range) ** 2

            if a.ndim == 3:
                ssim_vals = []
                for c in range(a.shape[-1]):
                    im1 = a[..., c].astype(np.float64)
                    im2 = b[..., c].astype(np.float64)
                    mu1 = ndimage.gaussian_filter(im1, 1.5)
                    mu2 = ndimage.gaussian_filter(im2, 1.5)
                    mu1_sq = mu1 * mu1
                    mu2_sq = mu2 * mu2
                    mu1_mu2 = mu1 * mu2
                    sigma1_sq = ndimage.gaussian_filter(im1 * im1, 1.5) - mu1_sq
                    sigma2_sq = ndimage.gaussian_filter(im2 * im2, 1.5) - mu2_sq
                    sigma12 = ndimage.gaussian_filter(im1 * im2, 1.5) - mu1_mu2
                    ssim_map = ((2 * mu1_mu2 + c1) * (2 * sigma12 + c2)) / (
                        (mu1_sq + mu2_sq + c1) * (sigma1_sq + sigma2_sq + c2)
                    )
                    ssim_vals.append(np.mean(ssim_map))
                return float(np.mean(ssim_vals))
            else:
                im1 = a.astype(np.float64)
                im2 = b.astype(np.float64)
                mu1 = ndimage.gaussian_filter(im1, 1.5)
                mu2 = ndimage.gaussian_filter(im2, 1.5)
                mu1_sq = mu1 * mu1
                mu2_sq = mu2 * mu2
                mu1_mu2 = mu1 * mu2
                sigma1_sq = ndimage.gaussian_filter(im1 * im1, 1.5) - mu1_sq
                sigma2_sq = ndimage.gaussian_filter(im2 * im2, 1.5) - mu2_sq
                sigma12 = ndimage.gaussian_filter(im1 * im2, 1.5) - mu1_mu2
                ssim_map = ((2 * mu1_mu2 + c1) * (2 * sigma12 + c2)) / (
                    (mu1_sq + mu2_sq + c1) * (sigma1_sq + sigma2_sq + c2)
                )
                return float(np.mean(ssim_map))
        except ImportError:
            # Basic fallback
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

